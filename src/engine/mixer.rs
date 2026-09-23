use std::f32::consts::FRAC_PI_2;

use crate::dsp::StereoFrame;

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
