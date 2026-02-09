use crate::effects::Effect;

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
    // One-pole lowpass state for tone filter
    tone_z1: f32,
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
            tone_z1: 0.0,
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
                // Asymmetric soft clipping: positive side clips softer than negative.
                // Models tube amp behavior where one half of the waveform is compressed
                // more than the other, generating even-order harmonics.
                if x >= 0.0 {
                    (2.0 * x).tanh() * 0.5
                } else {
                    (3.0 * x).tanh() * 0.333
                }
            }
            DistortionMode::Fuzz => {
                // Extreme clipping with a sharp sigmoid — nearly square wave at high drive.
                // The expm1 curve gives a gated, sputtery fuzz character.
                let sign = x.signum();
                let abs = x.abs();
                sign * (1.0 - (-5.0 * abs).exp())
            }
            DistortionMode::Saturate => {
                // Cubic soft saturation: x - x^3/3 for |x| <= 1, clamped beyond.
                // Very gentle — good for subtle warming.
                if x.abs() > 1.0 {
                    x.signum() * (2.0 / 3.0)
                } else {
                    x - (x * x * x) / 3.0
                }
            }
        }
    }
}

impl Effect for Distortion {
    fn process(&mut self, sample: f32, _sample_rate: f32) -> f32 {
        // Apply drive and bias
        let driven = self.drive * sample + self.bias;

        // Waveshape
        let shaped = self.waveshape(driven);

        // Remove the DC offset introduced by bias
        let shaped = shaped - self.waveshape(self.bias);

        // Tone filter: one-pole lowpass
        // Map tone 0..1 to coefficient. At tone=1.0 the filter is fully open (no filtering).
        // At tone=0.0 the cutoff is very low for a dark, fat sound.
        let coeff = (0.001 + self.tone * 0.999).powi(2);
        self.tone_z1 = self.tone_z1 + coeff * (shaped - self.tone_z1);
        let filtered = self.tone_z1;

        // Apply output gain
        let wet = filtered * self.output_gain;

        // Dry/wet mix
        sample * (1.0 - self.mix) + wet * self.mix
    }

    fn reset(&mut self) {
        self.tone_z1 = 0.0;
    }
}
