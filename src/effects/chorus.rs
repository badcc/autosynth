use std::f32::consts::PI;

use crate::effects::Effect;

/// Modulated delay chorus effect.
pub struct Chorus {
    buffer: Vec<f32>,
    write_pos: usize,
    lfo_phase: f32,
    rate: f32,
    depth: f32,
    mix: f32,
}

impl Chorus {
    /// Create a new chorus effect.
    ///
    /// `rate`: LFO rate in Hz (typical: 0.5–5.0)
    /// `depth`: modulation depth in seconds (typical: 0.001–0.01)
    /// `mix`: dry/wet mix (0.0 = fully dry, 1.0 = fully wet)
    /// `sample_rate`: audio sample rate in Hz
    pub fn new(rate: f32, depth: f32, mix: f32, sample_rate: f32) -> Self {
        // Buffer needs to hold the max delay (depth) plus some headroom
        let max_delay_samples = (depth * sample_rate * 2.0) as usize + 2;
        let buffer_size = max_delay_samples.max(4);
        Self {
            buffer: vec![0.0; buffer_size],
            write_pos: 0,
            lfo_phase: 0.0,
            rate: rate.max(0.01),
            depth: depth.max(0.0),
            mix: mix.clamp(0.0, 1.0),
        }
    }
}

impl Effect for Chorus {
    fn process(&mut self, sample: f32, sample_rate: f32) -> f32 {
        // Write input to buffer
        self.buffer[self.write_pos] = sample;

        // LFO modulates delay time
        let lfo = (2.0 * PI * self.lfo_phase).sin();
        self.lfo_phase = (self.lfo_phase + self.rate / sample_rate) % 1.0;

        // Delay time in samples, centered around depth
        let delay_samples = self.depth * sample_rate * (1.0 + lfo * 0.5);
        let delay_int = delay_samples as usize;
        let frac = delay_samples - delay_int as f32;

        // Linear interpolation between two adjacent samples
        let len = self.buffer.len();
        let pos_a = (self.write_pos + len - delay_int) % len;
        let pos_b = (self.write_pos + len - delay_int - 1) % len;

        let delayed = self.buffer[pos_a] * (1.0 - frac) + self.buffer[pos_b] * frac;

        self.write_pos = (self.write_pos + 1) % len;

        sample * (1.0 - self.mix) + delayed * self.mix
    }

    fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.write_pos = 0;
        self.lfo_phase = 0.0;
    }
}
