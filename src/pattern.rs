#[derive(Clone, Debug)]
pub struct PatternNote {
    pub beat: f32,
    pub note: u8,
    pub velocity: f32,
    pub duration: f32,
}

#[derive(Clone, Debug, Default)]
pub struct Pattern {
    notes: Vec<PatternNote>,
}

impl Pattern {
    pub fn new() -> Self {
        Self { notes: Vec::new() }
    }

    pub fn note(mut self, beat: f32, note: u8, vel: f32, dur: f32) -> Self {
        self.notes.push(PatternNote {
            beat,
            note,
            velocity: vel,
            duration: dur,
        });
        self
    }

    pub fn chord(mut self, beat: f32, notes: &[u8], vel: f32, dur: f32) -> Self {
        for &note in notes {
            self.notes.push(PatternNote {
                beat,
                note,
                velocity: vel,
                duration: dur,
            });
        }
        self
    }

    pub fn notes(&self) -> &[PatternNote] {
        &self.notes
    }

    pub fn length(&self) -> f32 {
        self.notes
            .iter()
            .map(|n| n.beat + n.duration)
            .fold(0.0, f32::max)
    }

    pub fn repeat(self, times: usize) -> Self {
        if times <= 1 {
            return self;
        }
        let len = self.length();
        let mut result = self.notes.clone();
        for i in 1..times {
            for note in &self.notes {
                result.push(PatternNote {
                    beat: note.beat + len * i as f32,
                    note: note.note,
                    velocity: note.velocity,
                    duration: note.duration,
                });
            }
        }
        Pattern { notes: result }
    }

    pub fn transpose(mut self, semitones: i8) -> Self {
        for note in &mut self.notes {
            note.note = (note.note as i16 + semitones as i16).clamp(0, 127) as u8;
        }
        self
    }

    pub fn shift(mut self, beats: f32) -> Self {
        for note in &mut self.notes {
            note.beat += beats;
        }
        self
    }

    pub fn then(self, other: Pattern) -> Self {
        let offset = self.length();
        self.layer(other.shift(offset))
    }

    pub fn layer(mut self, other: Pattern) -> Self {
        self.notes.extend(other.notes);
        self
    }
}

pub fn arp(notes: &[u8], step: f32, dur: f32, vel: f32) -> Pattern {
    let mut pattern = Pattern::new();
    for (i, &note) in notes.iter().enumerate() {
        pattern = pattern.note(i as f32 * step, note, vel, dur);
    }
    pattern
}

pub fn seq(notes: &[u8], step: f32, dur: f32, vel: f32) -> Pattern {
    arp(notes, step, dur, vel)
}

pub fn euclidean(hits: usize, steps: usize, note: u8, vel: f32, step_dur: f32) -> Pattern {
    if steps == 0 || hits == 0 {
        return Pattern::new();
    }
    let hits = hits.min(steps);
    let rhythm = compute_euclidean(hits, steps);
    let mut pattern = Pattern::new();
    for (i, &hit) in rhythm.iter().enumerate() {
        if hit {
            pattern = pattern.note(i as f32 * step_dur, note, vel, step_dur * 0.8);
        }
    }
    pattern
}

fn compute_euclidean(hits: usize, steps: usize) -> Vec<bool> {
    if hits >= steps {
        return vec![true; steps];
    }
    if hits == 0 {
        return vec![false; steps];
    }

    let mut pattern = Vec::with_capacity(steps);
    let mut counts = vec![1usize; hits];
    let mut remainders = vec![1usize; steps - hits];

    loop {
        if remainders.is_empty() || remainders.len() <= 1 {
            break;
        }

        let mut new_counts = Vec::new();
        let mut new_remainders = Vec::new();

        let pairs = counts.len().min(remainders.len());
        for _ in 0..pairs {
            new_counts.push(counts.pop().unwrap() + remainders.pop().unwrap());
        }

        new_remainders.extend(counts.drain(..));
        new_remainders.extend(remainders.drain(..));

        counts = new_counts;
        remainders = new_remainders;
    }

    counts.extend(remainders);

    for count in counts {
        pattern.push(true);
        for _ in 1..count {
            pattern.push(false);
        }
    }

    pattern
}
