use crate::dsp::StereoFrame;
use crate::dsp::effects::{Effect, FxCtx, ParamDef, mix};

pub const DRIVE: usize = 0;
pub const MIX: usize = 1;
pub const BIAS: usize = 2;
pub const TONE: usize = 3;
pub const OUTPUT: usize = 4;

pub const PARAMS: &[ParamDef] = &[
    ParamDef { name: "drive", default: 1.0 },
    ParamDef { name: "mix", default: 1.0 },
    ParamDef { name: "bias", default: 0.0 },
    ParamDef { name: "tone", default: 1.0 },
    ParamDef { name: "output", default: 1.0 },
];

/// Waveshaping curve.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DistortionMode {
    /// `tanh` soft clip — warm, musical saturation.
    SoftClip,
    /// Hard clip at ±1 — aggressive, buzzy.
    HardClip,
    /// Asymmetric tube-style — even harmonics, fat.
    Tube,
    /// Extreme fuzz — squared-off, broken-speaker character.
    Fuzz,
    /// Cubic soft saturation — gentle coloring.
    Saturate,
}

/// Drive → waveshaper (with bias for asymmetry) → one-pole tone → output gain.
pub struct Distortion {
    mode: DistortionMode,
    drive: f32,
    mix: f32,
    bias: f32,
    tone: f32,
    output: f32,
    z: [f32; 2],
}

impl Distortion {
    pub fn new(mode: DistortionMode) -> Self {
        Self {
            mode,
            drive: PARAMS[DRIVE].default,
            mix: PARAMS[MIX].default,
            bias: PARAMS[BIAS].default,
            tone: PARAMS[TONE].default,
            output: PARAMS[OUTPUT].default,
            z: [0.0; 2],
        }
    }

    #[inline]
    fn shape(&self, x: f32) -> f32 {
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
            DistortionMode::Fuzz => x.signum() * (1.0 - (-5.0 * x.abs()).exp()),
            DistortionMode::Saturate => {
                if x.abs() > 1.0 {
                    x.signum() * (2.0 / 3.0)
                } else {
                    x - x * x * x / 3.0
                }
            }
        }
    }
}

impl Effect for Distortion {
    fn process(&mut self, buf: &mut [StereoFrame], _ctx: &FxCtx) {
        let coeff = (0.001 + self.tone * 0.999).powi(2);
        let offset = self.shape(self.bias);
        for frame in buf.iter_mut() {
            for ch in 0..2 {
                let dry = frame[ch];
                let shaped = self.shape(self.drive * dry + self.bias) - offset;
                self.z[ch] += coeff * (shaped - self.z[ch]);
                frame[ch] = mix(dry, self.z[ch] * self.output, self.mix);
            }
        }
    }

    fn set(&mut self, slot: usize, v: f32) {
        match slot {
            DRIVE => self.drive = v.max(0.0),
            MIX => self.mix = v.clamp(0.0, 1.0),
            BIAS => self.bias = v.clamp(-1.0, 1.0),
            TONE => self.tone = v.clamp(0.0, 1.0),
            OUTPUT => self.output = v.max(0.0),
            _ => {}
        }
    }

    fn reset(&mut self) {
        self.z = [0.0; 2];
    }
}
