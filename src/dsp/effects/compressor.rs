use crate::dsp::StereoFrame;
use crate::dsp::effects::{Effect, FxCtx, ParamDef, mix};
use crate::dsp::{db_to_gain, gain_to_db};

pub const THRESHOLD: usize = 0;
pub const RATIO: usize = 1;
pub const ATTACK: usize = 2;
pub const RELEASE: usize = 3;
pub const MAKEUP: usize = 4;
pub const MIX: usize = 5;

pub const PARAMS: &[ParamDef] = &[
    ParamDef { name: "threshold", default: -18.0 },
    ParamDef { name: "ratio", default: 4.0 },
    ParamDef { name: "attack", default: 0.01 },
    ParamDef { name: "release", default: 0.12 },
    ParamDef { name: "makeup", default: 0.0 },
    ParamDef { name: "mix", default: 1.0 },
];

/// Feed-forward, stereo-linked compressor with a smoothed dB-domain gain
/// computer. `mix` below 1 gives parallel ("New York") compression.
pub struct Compressor {
    threshold: f32,
    ratio: f32,
    attack: f32,
    release: f32,
    makeup: f32,
    mix: f32,
    /// Current gain reduction in dB (≥ 0).
    reduction: f32,
}

impl Compressor {
    pub fn new() -> Self {
        Self {
            threshold: PARAMS[THRESHOLD].default,
            ratio: PARAMS[RATIO].default,
            attack: PARAMS[ATTACK].default,
            release: PARAMS[RELEASE].default,
            makeup: PARAMS[MAKEUP].default,
            mix: PARAMS[MIX].default,
            reduction: 0.0,
        }
    }

    /// Current gain reduction in dB — for tests and meters.
    pub fn reduction_db(&self) -> f32 {
        self.reduction
    }
}

impl Default for Compressor {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for Compressor {
    fn process(&mut self, buf: &mut [StereoFrame], ctx: &FxCtx) {
        let sr = ctx.sample_rate;
        let att = 1.0 - (-1.0 / (self.attack.max(1e-4) * sr)).exp();
        let rel = 1.0 - (-1.0 / (self.release.max(1e-3) * sr)).exp();
        let makeup = db_to_gain(self.makeup);
        for frame in buf.iter_mut() {
            let level = gain_to_db(frame[0].abs().max(frame[1].abs()));
            let over = level - self.threshold;
            let target = if over > 0.0 { over * (1.0 - 1.0 / self.ratio) } else { 0.0 };
            let coeff = if target > self.reduction { att } else { rel };
            self.reduction += (target - self.reduction) * coeff;
            let g = db_to_gain(-self.reduction) * makeup;
            frame[0] = mix(frame[0], frame[0] * g, self.mix);
            frame[1] = mix(frame[1], frame[1] * g, self.mix);
        }
    }

    fn set(&mut self, slot: usize, v: f32) {
        match slot {
            THRESHOLD => self.threshold = v.min(0.0),
            RATIO => self.ratio = v.max(1.0),
            ATTACK => self.attack = v.max(0.0),
            RELEASE => self.release = v.max(0.0),
            MAKEUP => self.makeup = v,
            MIX => self.mix = v.clamp(0.0, 1.0),
            _ => {}
        }
    }

    fn reset(&mut self) {
        self.reduction = 0.0;
    }
}
