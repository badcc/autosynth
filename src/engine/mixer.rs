use std::f32::consts::FRAC_PI_2;

use crate::dsp::StereoFrame;
use crate::dsp::effects::Effect;
use crate::dsp::smooth::Smoothed;

/// Transparent soft limiter. Linear below the threshold, then compresses toward
/// ±1.0 — replaces the old per-synth and per-session hard `clamp`, so loud
/// mixes round over instead of splintering.
#[inline]
pub fn soft_limit(x: f32) -> f32 {
    const T: f32 = 0.8;
    let a = x.abs();
    if a <= T {
        x
    } else {
        x.signum() * (T + (1.0 - T) * (((a - T) / (1.0 - T)).tanh()))
    }
}

/// Equal-power pan of a mono sample into a stereo frame. `pan` is `-1.0..1.0`.
#[inline]
pub fn pan_mono(m: f32, pan: f32) -> StereoFrame {
    let theta = (pan.clamp(-1.0, 1.0) + 1.0) * 0.5 * FRAC_PI_2;
    [m * theta.cos(), m * theta.sin()]
}

/// A summing bus with its own gain and effect chain. Used for groups (several
/// tracks sharing fx) and could back the master. Tracks feed pre-summed stereo
/// buffers in; the bus applies gain then its fx chain in place.
pub struct Bus {
    pub gain: Smoothed,
    pub fx: Vec<Box<dyn Effect>>,
    pub fx_enabled: Vec<bool>,
    pub members: Vec<String>,
}

impl Bus {
    pub fn new(gain: f32, sample_rate: f32, members: Vec<String>) -> Self {
        Self {
            gain: Smoothed::new(gain, 8.0, sample_rate),
            fx: Vec::new(),
            fx_enabled: Vec::new(),
            members,
        }
    }

    /// Apply gain and the fx chain to `buf` in place.
    pub fn process(&mut self, buf: &mut [StereoFrame], sample_rate: f32) {
        for frame in buf.iter_mut() {
            let g = self.gain.next();
            let mut f = [frame[0] * g, frame[1] * g];
            for (fx, en) in self.fx.iter_mut().zip(self.fx_enabled.iter()) {
                if *en {
                    f = fx.process(f, sample_rate);
                }
            }
            *frame = f;
        }
    }
}
