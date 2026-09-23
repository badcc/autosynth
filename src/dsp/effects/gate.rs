use crate::dsp::StereoFrame;
use crate::dsp::effects::{Effect, FxCtx, ParamDef};

pub const DEPTH: usize = 0;
pub const GRID: usize = 1;
pub const SMOOTH: usize = 2;

pub const PARAMS: &[ParamDef] = &[
    ParamDef { name: "depth", default: 1.0 },
    ParamDef { name: "grid", default: 0.25 },
    ParamDef { name: "smooth", default: 0.004 },
];

/// Trance gate: a beat-synced on/off step pattern chopping the signal.
/// `x`/`X` open the gate for a step, anything else closes it. Steps are `grid`
/// beats long and locked to song position, so the chop stays on the beat.
pub struct Gate {
    steps: Vec<bool>,
    depth: f32,
    grid: f32,
    smooth: f32,
    level: f32,
}

impl Gate {
    pub fn new(pattern: &str) -> Self {
        let steps: Vec<bool> = pattern.chars().filter(|c| !c.is_whitespace() && *c != '|').map(|c| c == 'x' || c == 'X').collect();
        Self {
            steps,
            depth: PARAMS[DEPTH].default,
            grid: PARAMS[GRID].default,
            smooth: PARAMS[SMOOTH].default,
            level: 1.0,
        }
    }
}

impl Effect for Gate {
    fn process(&mut self, buf: &mut [StereoFrame], ctx: &FxCtx) {
        if self.steps.is_empty() || self.grid <= 0.0 {
            return;
        }
        let coeff = 1.0 - (-1.0 / (self.smooth.max(1e-4) * ctx.sample_rate)).exp();
        let n = self.steps.len() as i64;
        for (i, frame) in buf.iter_mut().enumerate() {
            let beat = ctx.beat + i as f64 * ctx.beats_per_sample;
            let step = ((beat / self.grid as f64).floor() as i64).rem_euclid(n) as usize;
            let target = if self.steps[step] { 1.0 } else { 1.0 - self.depth };
            self.level += (target - self.level) * coeff;
            frame[0] *= self.level;
            frame[1] *= self.level;
        }
    }

    fn set(&mut self, slot: usize, v: f32) {
        match slot {
            DEPTH => self.depth = v.clamp(0.0, 1.0),
            GRID => self.grid = v.max(0.0),
            SMOOTH => self.smooth = v.max(0.0),
            _ => {}
        }
    }
}
