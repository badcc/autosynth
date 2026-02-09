use crate::effects::{Effect, StereoFrame};

/// Waveshaping algorithm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DistortionMode {
    /// Tanh soft clipping — warm, musical saturation.
    SoftClip,
    /// Hard clipping at ±1 — aggressive, buzzy, classic transistor sound.
    HardClip,
    /// Asymmetric tube-style saturation — even harmonics, warm fatness.
    Tube,
    /// Extreme fuzz — squared-off, gated, broken-speaker character.
    Fuzz,
    /// Cubic soft saturation — gentle, subtle coloring.
    Saturate,
}

/// Classic distortion effect with multiple waveshaping modes, tone shaping,
/// bias for asymmetric clipping, and output gain compensation.
pub struct Distortion {
    drive: f32,
    mix: f32,
    mode: DistortionMode,
    /// DC offset applied before waveshaping. Pushes the signal off-center
    /// to create asymmetric clipping, adding even harmonics for thickness.
    bias: f32,
    /// Post-distortion tone control. 0.0 = very dark/fat, 1.0 = fully bright.
    /// Internally this is a one-pole lowpass cutoff coefficient.
    tone: f32,
    /// Post-distortion output gain (linear). Use to tame volume after heavy drive.
    output_gain: f32,
    // One-pole lowpass state for tone filter (per channel)
    tone_z1: [f32; 2],
}

impl Distortion {
    /// Create a new distortion with soft clipping (default mode).
    ///
    /// `drive`: distortion amount (1.0 = mild, higher = more aggressive)
    /// `mix`: dry/wet mix (0.0 = fully dry, 1.0 = fully wet)
    pub fn new(drive: f32, mix: f32) -> Self {
        Self {
            drive: drive.max(0.0),
            mix: mix.clamp(0.0, 1.0),
            mode: DistortionMode::SoftClip,
            bias: 0.0,
            tone: 1.0,
            output_gain: 1.0,
            tone_z1: [0.0; 2],
        }
    }

    /// Set the distortion mode (waveshaping algorithm).
    pub fn mode(mut self, mode: DistortionMode) -> Self {
        self.mode = mode;
        self
    }

    /// Set bias for asymmetric clipping (0.0 = symmetric, try 0.1–0.4 for tube character).
    /// Adds even harmonics for a thicker, more complex sound.
    pub fn bias(mut self, bias: f32) -> Self {
        self.bias = bias.clamp(-1.0, 1.0);
        self
    }

    /// Set post-distortion tone (0.0 = dark/fat, 1.0 = bright/cutting).
    pub fn tone(mut self, tone: f32) -> Self {
        self.tone = tone.clamp(0.0, 1.0);
        self
    }

    /// Set output gain (linear). Useful for level compensation after heavy drive.
    pub fn output_gain(mut self, gain: f32) -> Self {
        self.output_gain = gain.max(0.0);
        self
    }

    /// Apply the selected waveshaping function to an input sample.
    fn waveshape(&self, x: f32) -> f32 {
        match self.mode {
            DistortionMode::SoftClip => x.tanh(),
            DistortionMode::HardClip => x.clamp(-1.0, 1.0),
            DistortionMode::Tube => {
                if x >= 0.0 {
                    (2.0 * x).tanh() * 0.5
                } else {
                    (3.0 * x).tanh() * 0.333
                }
            }
            DistortionMode::Fuzz => {
                let sign = x.signum();
                let abs = x.abs();
                sign * (1.0 - (-5.0 * abs).exp())
            }
            DistortionMode::Saturate => {
                if x.abs() > 1.0 {
                    x.signum() * (2.0 / 3.0)
                } else {
                    x - (x * x * x) / 3.0
                }
            }
        }
    }

    fn process_mono(&self, sample: f32, z1: &mut f32) -> f32 {
        let driven = self.drive * sample + self.bias;
        let shaped = self.waveshape(driven) - self.waveshape(self.bias);

        let coeff = (0.001 + self.tone * 0.999).powi(2);
        *z1 = *z1 + coeff * (shaped - *z1);

        let wet = *z1 * self.output_gain;
        sample * (1.0 - self.mix) + wet * self.mix
    }
}

impl Effect for Distortion {
    fn process(&mut self, frame: StereoFrame, _sample_rate: f32) -> StereoFrame {
        // Split out the state to avoid borrow issues
        let [mut z1_l, mut z1_r] = self.tone_z1;
        let l = self.process_mono(frame[0], &mut z1_l);
        let r = self.process_mono(frame[1], &mut z1_r);
        self.tone_z1 = [z1_l, z1_r];
        [l, r]
    }

    fn reset(&mut self) {
        self.tone_z1 = [0.0; 2];
    }
}
