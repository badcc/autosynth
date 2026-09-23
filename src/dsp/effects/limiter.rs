use crate::dsp::StereoFrame;
use crate::dsp::db_to_gain;
use crate::dsp::effects::{Effect, FxCtx, ParamDef};

pub const CEILING: usize = 0;
pub const RELEASE: usize = 1;

pub const PARAMS: &[ParamDef] = &[ParamDef { name: "ceiling", default: -0.3 }, ParamDef { name: "release", default: 0.08 }];

/// Peak limiter: instant attack, smooth release, never exceeds the ceiling.
pub struct Limiter {
    ceiling: f32,
    release: f32,
    gain: f32,
}

impl Limiter {
    pub fn new() -> Self {
        Self { ceiling: db_to_gain(PARAMS[CEILING].default), release: PARAMS[RELEASE].default, gain: 1.0 }
    }
}

impl Default for Limiter {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for Limiter {
    fn process(&mut self, buf: &mut [StereoFrame], ctx: &FxCtx) {
        let rel = 1.0 - (-1.0 / (self.release.max(1e-3) * ctx.sample_rate)).exp();
        for frame in buf.iter_mut() {
            let peak = frame[0].abs().max(frame[1].abs());
            let needed = if peak > self.ceiling { self.ceiling / peak } else { 1.0 };
            // Instant attack; release glides up but never past what this
            // sample allows.
            self.gain = (self.gain + (1.0 - self.gain) * rel).min(needed);
            frame[0] *= self.gain;
            frame[1] *= self.gain;
        }
    }

    fn set(&mut self, slot: usize, v: f32) {
        match slot {
            CEILING => self.ceiling = db_to_gain(v.min(0.0)),
            RELEASE => self.release = v.max(0.0),
            _ => {}
        }
    }

    fn reset(&mut self) {
        self.gain = 1.0;
    }
}
