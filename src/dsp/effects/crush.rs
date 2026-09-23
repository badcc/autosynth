use crate::dsp::StereoFrame;
use crate::dsp::effects::{Effect, FxCtx, ParamDef, mix};

pub const BITS: usize = 0;
pub const DOWN: usize = 1;
pub const MIX: usize = 2;

pub const PARAMS: &[ParamDef] = &[
    ParamDef { name: "bits", default: 8.0 },
    ParamDef { name: "down", default: 4.0 },
    ParamDef { name: "mix", default: 1.0 },
];

/// Bitcrusher: amplitude quantization plus sample-and-hold downsampling.
pub struct Crush {
    bits: f32,
    down: f32,
    mix: f32,
    held: StereoFrame,
    counter: f32,
}

impl Crush {
    pub fn new() -> Self {
        Self { bits: PARAMS[BITS].default, down: PARAMS[DOWN].default, mix: PARAMS[MIX].default, held: [0.0; 2], counter: 0.0 }
    }
}

impl Default for Crush {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for Crush {
    fn process(&mut self, buf: &mut [StereoFrame], _ctx: &FxCtx) {
        let levels = 2.0_f32.powf(self.bits - 1.0);
        for frame in buf.iter_mut() {
            self.counter += 1.0;
            if self.counter >= self.down {
                self.counter -= self.down;
                self.held = [(frame[0] * levels).round() / levels, (frame[1] * levels).round() / levels];
            }
            frame[0] = mix(frame[0], self.held[0], self.mix);
            frame[1] = mix(frame[1], self.held[1], self.mix);
        }
    }

    fn set(&mut self, slot: usize, v: f32) {
        match slot {
            BITS => self.bits = v.clamp(1.0, 24.0),
            DOWN => self.down = v.max(1.0),
            MIX => self.mix = v.clamp(0.0, 1.0),
            _ => {}
        }
    }

    fn reset(&mut self) {
        self.held = [0.0; 2];
        self.counter = 0.0;
    }
}
