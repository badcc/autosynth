use std::f32::consts::TAU;

/// Timing context passed to every automation closure.
///
/// A `Clock` is computed directly from the engine transport (never
/// reverse-engineered from a sample counter), so it stays correct across
/// tempo changes and loop wraps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Clock {
    /// Global beat position since playback start.
    pub beat: f32,
    /// Beat position within the current loop (`0..loop_len`).
    /// Equal to `beat` when the track is not looping.
    pub local: f32,
    /// Current loop iteration (0 on first play, increments each wrap).
    pub iteration: u32,
}

impl Clock {
    pub const ZERO: Clock = Clock {
        beat: 0.0,
        local: 0.0,
        iteration: 0,
    };

    /// Rising phase `0.0..1.0` over a window of `len` beats (global).
    ///
    /// `c.phase(16.0)` sweeps 0→1 across sixteen beats, then repeats.
    pub fn phase(&self, len: f32) -> f32 {
        if len <= 0.0 {
            return 0.0;
        }
        (self.beat / len).rem_euclid(1.0)
    }

    /// Linear ramp from `a` to `b` over `len` beats, repeating.
    pub fn ramp(&self, a: f32, b: f32, len: f32) -> f32 {
        a + (b - a) * self.phase(len)
    }

    /// Sine oscillation in `-1.0..1.0` with a period of `len` beats.
    ///
    /// `400.0 + 300.0 * c.sin(8.0)` is an eight-beat filter wobble.
    pub fn sin(&self, len: f32) -> f32 {
        if len <= 0.0 {
            return 0.0;
        }
        (self.beat / len * TAU).sin()
    }

    /// Triangle oscillation in `-1.0..1.0` with a period of `len` beats.
    pub fn tri(&self, len: f32) -> f32 {
        4.0 * (self.phase(len) - 0.5).abs() - 1.0
    }
}
