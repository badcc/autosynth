use crate::dsp::StereoFrame;
use crate::dsp::effects::{Effect, FxCtx, ParamDef};
use crate::dsp::filter::{Filter, FilterType};
use crate::dsp::smooth::Smoothed;

pub const CUTOFF: usize = 0;
pub const RES: usize = 1;

pub const PARAMS: &[ParamDef] = &[ParamDef { name: "cutoff", default: 1000.0 }, ParamDef { name: "res", default: 0.0 }];

/// A stereo filter sweep — the progressive-house highpass build, the dub
/// lowpass. Cutoff is smoothed per sample so automation never zippers.
pub struct FilterFx {
    kind: FilterType,
    filters: [Filter; 2],
    cutoff: Smoothed,
    res: f32,
}

impl FilterFx {
    pub fn new(sample_rate: f32, kind: FilterType) -> Self {
        Self { kind, filters: [Filter::default(); 2], cutoff: Smoothed::new(PARAMS[CUTOFF].default, 10.0, sample_rate), res: 0.0 }
    }
}

impl Effect for FilterFx {
    fn process(&mut self, buf: &mut [StereoFrame], ctx: &FxCtx) {
        for frame in buf.iter_mut() {
            let fc = self.cutoff.next();
            for ch in 0..2 {
                frame[ch] = self.filters[ch].process(frame[ch], self.kind, fc, self.res, ctx.sample_rate);
            }
        }
    }

    fn set(&mut self, slot: usize, v: f32) {
        match slot {
            CUTOFF => self.cutoff.set_target(v.clamp(20.0, 20_000.0)),
            RES => self.res = v.clamp(0.0, 1.0),
            _ => {}
        }
    }

    fn reset(&mut self) {
        for f in &mut self.filters {
            f.reset();
        }
    }
}
