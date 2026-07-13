//! Pure-data automation shapes. They cover the common LFO/ramp automations
//! without an annotated closure: `t.cutoff(sine(40.0).range(0.0, 1000.0))`
//! instead of `t.cutoff(|c: Clock| ...)`. Closures remain the escape hatch for
//! anything a shape can't express.

use std::f32::consts::TAU;

use crate::music::Clock;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Sine,
    Tri,
    Saw,
    Ramp,
}

/// A small `Copy` automation curve. Built with `sine`/`tri`/`saw`/`ramp`, then
/// optionally remapped (`.range`) and phase-shifted (`.phase`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shape {
    kind: Kind,
    period: f32,
    lo: f32,
    hi: f32,
    phase: f32,
}

impl Shape {
    const fn wave(kind: Kind, period: f32) -> Self {
        Self { kind, period, lo: -1.0, hi: 1.0, phase: 0.0 }
    }

    /// Remap the output range. `sine/tri/saw` default to `-1.0..1.0`; this maps
    /// that span onto `lo..hi`.
    pub fn range(mut self, lo: f32, hi: f32) -> Self {
        self.lo = lo;
        self.hi = hi;
        self
    }

    /// Offset the phase by a fraction of a period (`0.0..1.0`).
    pub fn phase(mut self, frac: f32) -> Self {
        self.phase += frac;
        self
    }

    /// Evaluate the shape at a clock position.
    pub fn eval(&self, c: Clock) -> f32 {
        if self.period <= 0.0 {
            return self.lo;
        }
        let p = (c.beat / self.period + self.phase).rem_euclid(1.0);
        // Normalized `0.0..1.0` position along the waveform.
        let unit = match self.kind {
            Kind::Sine => 0.5 + 0.5 * (p * TAU).sin(),
            Kind::Saw | Kind::Ramp => p,
            Kind::Tri => 2.0 * (p - 0.5).abs(),
        };
        self.lo + (self.hi - self.lo) * unit
    }
}

/// Sine oscillation in `-1.0..1.0` with a period of `period` beats.
pub fn sine(period: f32) -> Shape {
    Shape::wave(Kind::Sine, period)
}

/// Triangle oscillation in `-1.0..1.0` with a period of `period` beats.
pub fn tri(period: f32) -> Shape {
    Shape::wave(Kind::Tri, period)
}

/// Rising sawtooth in `-1.0..1.0` with a period of `period` beats.
pub fn saw(period: f32) -> Shape {
    Shape::wave(Kind::Saw, period)
}

/// Linear ramp from `a` to `b` over `len` beats, repeating.
pub fn ramp(a: f32, b: f32, len: f32) -> Shape {
    Shape { kind: Kind::Ramp, period: len, lo: a, hi: b, phase: 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(beat: f32) -> Clock {
        Clock { beat, local: beat, iteration: 0 }
    }

    #[test]
    fn sine_matches_clock_convention() {
        let sh = sine(4.0);
        assert!((sh.eval(at(0.0)) - 0.0).abs() < 1e-5, "starts at 0 rising");
        assert!((sh.eval(at(1.0)) - 1.0).abs() < 1e-5, "peak at quarter period");
        assert!((sh.eval(at(2.0)) - 0.0).abs() < 1e-5);
        assert!((sh.eval(at(3.0)) + 1.0).abs() < 1e-5, "trough at 3/4 period");
    }

    #[test]
    fn saw_rises_across_the_period() {
        let sh = saw(4.0);
        assert!((sh.eval(at(0.0)) + 1.0).abs() < 1e-5);
        assert!((sh.eval(at(2.0)) - 0.0).abs() < 1e-5);
        assert!(sh.eval(at(3.9)) > 0.9);
    }

    #[test]
    fn tri_peaks_at_edges() {
        let sh = tri(4.0);
        assert!((sh.eval(at(0.0)) - 1.0).abs() < 1e-5);
        assert!((sh.eval(at(2.0)) + 1.0).abs() < 1e-5);
    }

    #[test]
    fn ramp_is_linear_and_repeats() {
        let sh = ramp(100.0, 200.0, 4.0);
        assert!((sh.eval(at(0.0)) - 100.0).abs() < 1e-4);
        assert!((sh.eval(at(2.0)) - 150.0).abs() < 1e-4);
        assert!((sh.eval(at(4.0)) - 100.0).abs() < 1e-4, "wraps at the period");
    }

    #[test]
    fn range_maps_onto_new_span() {
        let sh = sine(4.0).range(0.0, 1000.0);
        assert!((sh.eval(at(0.0)) - 500.0).abs() < 1e-3, "mid of 0..1000");
        assert!((sh.eval(at(1.0)) - 1000.0).abs() < 1e-3, "peak");
        assert!((sh.eval(at(3.0)) - 0.0).abs() < 1e-3, "trough");
    }

    #[test]
    fn phase_shifts_the_waveform() {
        let base = saw(4.0);
        let shifted = saw(4.0).phase(0.25);
        assert!((shifted.eval(at(0.0)) - base.eval(at(1.0))).abs() < 1e-5);
    }
}
