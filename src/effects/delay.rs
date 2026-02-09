use crate::effects::Effect;

/// Simple feedback delay with circular buffer.
pub struct Delay {
    buffer: Vec<f32>,
    write_pos: usize,
    delay_samples: usize,
    feedback: f32,
    mix: f32,
}

impl Delay {
    /// Create a new delay effect.
    ///
    /// `delay_time`: delay time in seconds
    /// `feedback`: feedback amount (0.0 to ~0.95)
    /// `mix`: dry/wet mix (0.0 = fully dry, 1.0 = fully wet)
    /// `sample_rate`: audio sample rate in Hz
    pub fn new(delay_time: f32, feedback: f32, mix: f32, sample_rate: f32) -> Self {
        let delay_samples = (delay_time * sample_rate) as usize;
        let buffer_size = delay_samples.max(1);
        Self {
            buffer: vec![0.0; buffer_size],
            write_pos: 0,
            delay_samples: buffer_size,
            feedback: feedback.clamp(0.0, 0.95),
            mix: mix.clamp(0.0, 1.0),
        }
    }

    /// Create a tempo-synced delay.
    ///
    /// `delay_beats`: delay time in beats
    /// `bpm`: tempo in beats per minute
    /// `feedback`: feedback amount (0.0 to ~0.95)
    /// `mix`: dry/wet mix (0.0 = fully dry, 1.0 = fully wet)
    /// `sample_rate`: audio sample rate in Hz
    pub fn tempo_synced(
        delay_beats: f32,
        bpm: f32,
        feedback: f32,
        mix: f32,
        sample_rate: f32,
    ) -> Self {
        let delay_seconds = delay_beats * 60.0 / bpm;
        Self::new(delay_seconds, feedback, mix, sample_rate)
    }
}

impl Effect for Delay {
    fn process(&mut self, sample: f32, _sample_rate: f32) -> f32 {
        let read_pos = (self.write_pos + self.buffer.len() - self.delay_samples) % self.buffer.len();
        let delayed = self.buffer[read_pos];

        self.buffer[self.write_pos] = sample + delayed * self.feedback;
        self.write_pos = (self.write_pos + 1) % self.buffer.len();

        sample * (1.0 - self.mix) + delayed * self.mix
    }

    fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.write_pos = 0;
    }
}
