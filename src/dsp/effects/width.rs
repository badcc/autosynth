use crate::dsp::StereoFrame;
use crate::dsp::effects::{Effect, FxCtx, ParamDef};

pub const WIDTH: usize = 0;

pub const PARAMS: &[ParamDef] = &[ParamDef { name: "width", default: 1.0 }];

/// Mid/side stereo width: 0 = mono, 1 = unchanged, 2 = extra wide.
pub struct Width {
    width: f32,
}

impl Width {
    pub fn new() -> Self {
        Self { width: PARAMS[WIDTH].default }
    }
}

impl Default for Width {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for Width {
    fn process(&mut self, buf: &mut [StereoFrame], _ctx: &FxCtx) {
        for frame in buf.iter_mut() {
            let mid = (frame[0] + frame[1]) * 0.5;
            let side = (frame[0] - frame[1]) * 0.5 * self.width;
            *frame = [mid + side, mid - side];
        }
    }

    fn set(&mut self, slot: usize, v: f32) {
        if slot == WIDTH {
            self.width = v.clamp(0.0, 3.0);
        }
    }
}
