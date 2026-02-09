use std::f32::consts::PI;

use crate::effects::{Effect, StereoFrame};

/// Modulated delay chorus effect.
pub struct Chorus {
    buffers: [Vec<f32>; 2],
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
        let max_delay_samples = (depth * sample_rate * 2.0) as usize + 2;
        let buffer_size = max_delay_samples.max(4);
        Self {
            buffers: [vec![0.0; buffer_size], vec![0.0; buffer_size]],
            write_pos: 0,
            lfo_phase: 0.0,
            rate: rate.max(0.01),
            depth: depth.max(0.0),
            mix: mix.clamp(0.0, 1.0),
        }
    }

    fn process_channel(&self, buffer: &[f32], delay_samples: f32) -> f32 {
        let delay_int = delay_samples as usize;
        let frac = delay_samples - delay_int as f32;

        let len = buffer.len();
        let pos_a = (self.write_pos + len - delay_int) % len;
        let pos_b = (self.write_pos + len - delay_int - 1) % len;

        buffer[pos_a] * (1.0 - frac) + buffer[pos_b] * frac
    }
}

impl Effect for Chorus {
    fn process(&mut self, frame: StereoFrame, sample_rate: f32) -> StereoFrame {
        // Write input to buffers
        self.buffers[0][self.write_pos] = frame[0];
        self.buffers[1][self.write_pos] = frame[1];

        // Shared LFO
        let lfo = (2.0 * PI * self.lfo_phase).sin();
        self.lfo_phase = (self.lfo_phase + self.rate / sample_rate) % 1.0;

        let delay_samples = self.depth * sample_rate * (1.0 + lfo * 0.5);

        let delayed_l = self.process_channel(&self.buffers[0], delay_samples);
        let delayed_r = self.process_channel(&self.buffers[1], delay_samples);

        self.write_pos = (self.write_pos + 1) % self.buffers[0].len();

        [
            frame[0] * (1.0 - self.mix) + delayed_l * self.mix,
            frame[1] * (1.0 - self.mix) + delayed_r * self.mix,
        ]
    }

    fn reset(&mut self) {
        self.buffers[0].fill(0.0);
        self.buffers[1].fill(0.0);
        self.write_pos = 0;
        self.lfo_phase = 0.0;
    }
}
