/// Beat-native clock with a movable song position.
///
/// `beat` is monotonic engine time: schedulers anchor to it, so running loops
/// never jump. The *song* position — what signals, curves and phrases read —
/// is `beat + offset`, and `jump`/`hold` move it on bar boundaries. Because
/// the offset is always a whole number of bars, loops stay in phase with the
/// song grid across any jump.
#[derive(Clone, Copy, Debug)]
pub struct Transport {
    pub beat: f64,
    pub bpm: f64,
    pub sample_rate: f64,
    offset: f64,
    /// Pending jump: at engine beat `.0`, the song moves to beat `.1`.
    jump: Option<(f64, f64)>,
    /// Loop the song between these song beats.
    hold: Option<(f64, f64)>,
}

impl Transport {
    pub fn new(bpm: f32, sample_rate: f32) -> Self {
        Self { beat: 0.0, bpm: bpm as f64, sample_rate: sample_rate as f64, offset: 0.0, jump: None, hold: None }
    }

    #[inline]
    pub fn beats_per_sample(&self) -> f64 {
        self.bpm / (60.0 * self.sample_rate)
    }

    #[inline]
    pub fn beat_at(&self, frames: usize) -> f64 {
        self.beat + frames as f64 * self.beats_per_sample()
    }

    #[inline]
    pub fn advance(&mut self, frames: usize) {
        self.beat += frames as f64 * self.beats_per_sample();
    }

    /// The next bar line at or after the current beat.
    pub fn next_bar(&self) -> f64 {
        if self.beat <= 0.0 { 0.0 } else { (self.beat / 4.0).ceil() * 4.0 }
    }

    /// Song position at engine beat `g`, accounting for a pending jump and an
    /// active hold loop.
    pub fn song(&self, g: f64) -> f64 {
        let offset = match self.jump {
            Some((at, target)) if g >= at => target - at,
            _ => self.offset,
        };
        let s = g + offset;
        match self.hold {
            Some((a, b)) if b > a && s >= b => a + (s - a).rem_euclid(b - a),
            _ => s,
        }
    }

    /// Move the song to `song_beat` at the next bar line.
    pub fn jump(&mut self, song_beat: f64) {
        self.jump = Some((self.next_bar(), song_beat));
    }

    /// Loop the song between two song beats (or stop looping).
    pub fn hold(&mut self, span: Option<(f64, f64)>) {
        self.offset = self.song(self.beat) - self.beat;
        self.hold = span;
    }

    /// Bake pending jumps and hold wraps into the offset. Call once per block.
    pub fn update(&mut self) {
        let s = self.song(self.beat);
        if matches!(self.jump, Some((at, _)) if self.beat >= at) {
            self.jump = None;
        }
        self.offset = s - self.beat;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jump_lands_on_the_next_bar() {
        let mut t = Transport::new(120.0, 48_000.0);
        t.beat = 5.0;
        t.jump(64.0);
        assert_eq!(t.song(7.9), 7.9, "before the bar line nothing moves");
        assert_eq!(t.song(8.0), 64.0);
        t.beat = 9.0;
        t.update();
        assert_eq!(t.song(9.0), 65.0);
        assert_eq!(t.song(10.0), 66.0);
    }

    #[test]
    fn hold_loops_and_release_continues() {
        let mut t = Transport::new(120.0, 48_000.0);
        t.hold(Some((4.0, 8.0)));
        assert_eq!(t.song(9.0), 5.0);
        t.beat = 9.0;
        t.update();
        t.hold(None);
        assert_eq!(t.song(10.0), 6.0, "releasing continues from where the loop was");
    }
}
