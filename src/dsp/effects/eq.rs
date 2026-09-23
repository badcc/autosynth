use std::f32::consts::PI;

use crate::dsp::StereoFrame;
use crate::dsp::db_to_gain;
use crate::dsp::effects::{Effect, FxCtx, ParamDef};

pub const LOW: usize = 0;
pub const MID: usize = 1;
pub const HIGH: usize = 2;
pub const LOW_FREQ: usize = 3;
pub const MID_FREQ: usize = 4;
pub const HIGH_FREQ: usize = 5;

pub const PARAMS: &[ParamDef] = &[
    ParamDef { name: "low", default: 0.0 },
    ParamDef { name: "mid", default: 0.0 },
    ParamDef { name: "high", default: 0.0 },
    ParamDef { name: "low_freq", default: 200.0 },
    ParamDef { name: "mid_freq", default: 1000.0 },
    ParamDef { name: "high_freq", default: 5000.0 },
];

#[derive(Clone, Copy, Default)]
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z: [[f32; 2]; 2],
}

enum Shape {
    LowShelf,
    Peak,
    HighShelf,
}

impl Biquad {
    /// RBJ cookbook coefficients.
    fn design(&mut self, shape: Shape, freq: f32, db: f32, sample_rate: f32) {
        let a = db_to_gain(db / 2.0);
        let w = 2.0 * PI * freq.clamp(20.0, sample_rate * 0.45) / sample_rate;
        let (sin, cos) = w.sin_cos();
        let (b0, b1, b2, a0, a1, a2) = match shape {
            Shape::Peak => {
                let alpha = sin / (2.0 * 0.7);
                (1.0 + alpha * a, -2.0 * cos, 1.0 - alpha * a, 1.0 + alpha / a, -2.0 * cos, 1.0 - alpha / a)
            }
            Shape::LowShelf | Shape::HighShelf => {
                let alpha = sin / 2.0 * 2.0_f32.sqrt();
                let sq = 2.0 * a.sqrt() * alpha;
                let s = if matches!(shape, Shape::LowShelf) { 1.0 } else { -1.0 };
                (
                    a * ((a + 1.0) - s * (a - 1.0) * cos + sq),
                    s * 2.0 * a * ((a - 1.0) - s * (a + 1.0) * cos),
                    a * ((a + 1.0) - s * (a - 1.0) * cos - sq),
                    (a + 1.0) + s * (a - 1.0) * cos + sq,
                    -s * 2.0 * ((a - 1.0) + s * (a + 1.0) * cos),
                    (a + 1.0) + s * (a - 1.0) * cos - sq,
                )
            }
        };
        self.b0 = b0 / a0;
        self.b1 = b1 / a0;
        self.b2 = b2 / a0;
        self.a1 = a1 / a0;
        self.a2 = a2 / a0;
    }

    #[inline]
    fn process(&mut self, ch: usize, x: f32) -> f32 {
        let z = &mut self.z[ch];
        let y = self.b0 * x + z[0];
        z[0] = self.b1 * x - self.a1 * y + z[1];
        z[1] = self.b2 * x - self.a2 * y;
        y
    }
}

/// Three-band EQ: low shelf, mid peak, high shelf (gains in dB).
pub struct Eq {
    bands: [Biquad; 3],
    values: [f32; 6],
    dirty: bool,
}

impl Eq {
    pub fn new() -> Self {
        let mut values = [0.0; 6];
        for (v, p) in values.iter_mut().zip(PARAMS) {
            *v = p.default;
        }
        Self { bands: [Biquad::default(); 3], values, dirty: true }
    }
}

impl Default for Eq {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for Eq {
    fn process(&mut self, buf: &mut [StereoFrame], ctx: &FxCtx) {
        if self.dirty {
            let v = self.values;
            self.bands[0].design(Shape::LowShelf, v[LOW_FREQ], v[LOW], ctx.sample_rate);
            self.bands[1].design(Shape::Peak, v[MID_FREQ], v[MID], ctx.sample_rate);
            self.bands[2].design(Shape::HighShelf, v[HIGH_FREQ], v[HIGH], ctx.sample_rate);
            self.dirty = false;
        }
        for frame in buf.iter_mut() {
            for ch in 0..2 {
                let mut x = frame[ch];
                for b in &mut self.bands {
                    x = b.process(ch, x);
                }
                frame[ch] = x;
            }
        }
    }

    fn set(&mut self, slot: usize, v: f32) {
        if let Some(x) = self.values.get_mut(slot)
            && *x != v
        {
            *x = v;
            self.dirty = true;
        }
    }

    fn reset(&mut self) {
        for b in &mut self.bands {
            b.z = [[0.0; 2]; 2];
        }
    }
}
