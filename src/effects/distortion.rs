use crate::effects::Effect;

/// Soft-clip waveshaper distortion using tanh.
pub struct Distortion {
    drive: f32,
    mix: f32,
}

impl Distortion {
    /// Create a new distortion effect.
    ///
    /// `drive`: distortion amount (1.0 = mild, higher = more aggressive)
    /// `mix`: dry/wet mix (0.0 = fully dry, 1.0 = fully wet)
    pub fn new(drive: f32, mix: f32) -> Self {
        Self {
            drive: drive.max(0.0),
            mix: mix.clamp(0.0, 1.0),
        }
    }
}

impl Effect for Distortion {
    fn process(&mut self, sample: f32, _sample_rate: f32) -> f32 {
        let distorted = (self.drive * sample).tanh();
        sample * (1.0 - self.mix) + distorted * self.mix
    }

    fn reset(&mut self) {}
}
