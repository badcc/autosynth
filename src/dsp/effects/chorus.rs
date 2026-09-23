use std::f32::consts::TAU;

use crate::dsp::StereoFrame;
use crate::dsp::effects::{Effect, FxCtx, ParamDef, mix};

pub const RATE: usize = 0;
pub const DEPTH: usize = 1;
pub const MIX: usize = 2;

pub const PARAMS: &[ParamDef] = &[
    ParamDef { name: "rate", default: 0.8 },
    ParamDef { name: "depth", default: 0.004 },
    ParamDef { name: "mix", default: 0.4 },
];

const MAX_SECONDS: f32 = 0.05;
const BASE_SECONDS: f32 = 0.012;

/// Stereo chorus: a modulated delay line per channel, LFOs in quadrature.
pub struct Chorus {
    buf: [Vec<f32>; 2],
    write: usize,
    phase: f32,
    rate: f32,
    depth: f32,
    mix: f32,
}

impl Chorus {
    pub fn new(sample_rate: f32) -> Self {
        let len = (MAX_SECONDS * sample_rate) as usize + 2;
        Self {
            buf: [vec![0.0; len], vec![0.0; len]],
            write: 0,
            phase: 0.0,
            rate: PARAMS[RATE].default,
            depth: PARAMS[DEPTH].default,
            mix: PARAMS[MIX].default,
        }
    }

    fn read(&self, ch: usize, delay: f32) -> f32 {
        let len = self.buf[ch].len();
        let pos = (self.write as f32 - delay).rem_euclid(len as f32);
        let i = pos as usize % len;
        let frac = pos - pos.floor();
        self.buf[ch][i] * (1.0 - frac) + self.buf[ch][(i + 1) % len] * frac
    }
}

impl Effect for Chorus {
    fn process(&mut self, buf: &mut [StereoFrame], ctx: &FxCtx) {
        let sr = ctx.sample_rate;
        let len = self.buf[0].len();
        let max_delay = (len - 2) as f32;
        for frame in buf.iter_mut() {
            self.buf[0][self.write] = frame[0];
            self.buf[1][self.write] = frame[1];
            let l = (TAU * self.phase).sin();
            let r = (TAU * (self.phase + 0.25)).sin();
            self.phase = (self.phase + self.rate / sr).rem_euclid(1.0);
            let dl = ((BASE_SECONDS + self.depth * l) * sr).clamp(1.0, max_delay);
            let dr = ((BASE_SECONDS + self.depth * r) * sr).clamp(1.0, max_delay);
            let (wl, wr) = (self.read(0, dl), self.read(1, dr));
            self.write = (self.write + 1) % len;
            frame[0] = mix(frame[0], wl, self.mix);
            frame[1] = mix(frame[1], wr, self.mix);
        }
    }

    fn set(&mut self, slot: usize, v: f32) {
        match slot {
            RATE => self.rate = v.max(0.0),
            DEPTH => self.depth = v.clamp(0.0, 0.01),
            MIX => self.mix = v.clamp(0.0, 1.0),
            _ => {}
        }
    }

    fn reset(&mut self) {
        self.buf[0].fill(0.0);
        self.buf[1].fill(0.0);
    }
}
