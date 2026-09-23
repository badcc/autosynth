use std::f32::consts::{PI, TAU};

use crate::dsp::StereoFrame;
use crate::dsp::effects::{Effect, FxCtx, ParamDef, mix};

pub const RATE: usize = 0;
pub const DEPTH: usize = 1;
pub const FEEDBACK: usize = 2;
pub const MIX: usize = 3;

pub const PARAMS: &[ParamDef] = &[
    ParamDef { name: "rate", default: 0.3 },
    ParamDef { name: "depth", default: 0.7 },
    ParamDef { name: "feedback", default: 0.5 },
    ParamDef { name: "mix", default: 0.5 },
];

const STAGES: usize = 4;

/// Four first-order allpass stages swept by an LFO (200 Hz – 3 kHz), with
/// feedback. The right channel's LFO runs a quarter cycle ahead for width.
pub struct Phaser {
    z: [[f32; STAGES]; 2],
    fb: [f32; 2],
    phase: f32,
    rate: f32,
    depth: f32,
    feedback: f32,
    mix: f32,
}

impl Phaser {
    pub fn new() -> Self {
        Self {
            z: [[0.0; STAGES]; 2],
            fb: [0.0; 2],
            phase: 0.0,
            rate: PARAMS[RATE].default,
            depth: PARAMS[DEPTH].default,
            feedback: PARAMS[FEEDBACK].default,
            mix: PARAMS[MIX].default,
        }
    }
}

impl Default for Phaser {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for Phaser {
    fn process(&mut self, buf: &mut [StereoFrame], ctx: &FxCtx) {
        let sr = ctx.sample_rate;
        for frame in buf.iter_mut() {
            self.phase = (self.phase + self.rate / sr).rem_euclid(1.0);
            for ch in 0..2 {
                let lfo = 0.5 + 0.5 * (TAU * (self.phase + ch as f32 * 0.25)).sin();
                let fc = 200.0 * (15.0f32).powf(lfo * self.depth);
                let t = (PI * fc / sr).tan();
                let a = (t - 1.0) / (t + 1.0);
                let dry = frame[ch];
                let mut x = dry + self.fb[ch] * self.feedback;
                for s in &mut self.z[ch] {
                    let y = a * x + *s;
                    *s = x - a * y;
                    x = y;
                }
                self.fb[ch] = x;
                frame[ch] = mix(dry, x, self.mix);
            }
        }
    }

    fn set(&mut self, slot: usize, v: f32) {
        match slot {
            RATE => self.rate = v.max(0.0),
            DEPTH => self.depth = v.clamp(0.0, 1.0),
            FEEDBACK => self.feedback = v.clamp(-0.95, 0.95),
            MIX => self.mix = v.clamp(0.0, 1.0),
            _ => {}
        }
    }

    fn reset(&mut self) {
        self.z = [[0.0; STAGES]; 2];
        self.fb = [0.0; 2];
    }
}
