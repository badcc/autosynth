/// Beat-native clock. The engine advances `beat` by a fixed amount each block;
/// every downstream consumer reads beats, so a tempo change simply takes effect
/// on the next block — running loops, schedulers and automations all stay
/// correct without recomputing sample positions.
#[derive(Clone, Copy, Debug)]
pub struct Transport {
    pub beat: f64,
    pub bpm: f64,
    pub sample_rate: f64,
}

impl Transport {
    pub fn new(bpm: f32, sample_rate: f32) -> Self {
        Self {
            beat: 0.0,
            bpm: bpm as f64,
            sample_rate: sample_rate as f64,
        }
    }

    /// Beats elapsed per audio sample at the current tempo.
    #[inline]
    pub fn beats_per_sample(&self) -> f64 {
        self.bpm / (60.0 * self.sample_rate)
    }

    /// The beat position `frames` samples into the current block.
    #[inline]
    pub fn beat_at(&self, frames: usize) -> f64 {
        self.beat + frames as f64 * self.beats_per_sample()
    }

    /// Advance the global beat by `frames` samples.
    #[inline]
    pub fn advance(&mut self, frames: usize) {
        self.beat += frames as f64 * self.beats_per_sample();
    }

    /// Convert a beat duration to a (fractional) sample count at this tempo.
    #[inline]
    pub fn beats_to_samples(&self, beats: f64) -> f64 {
        beats / self.beats_per_sample()
    }
}
