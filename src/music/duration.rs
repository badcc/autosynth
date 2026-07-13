//! Duration helpers. All durations are in beats (a quarter note = `1.0`).
//! One name per concept — no aliases.

/// Quarter note (1 beat).
pub const fn q() -> f32 {
    1.0
}

/// Half note (2 beats).
pub const fn h() -> f32 {
    2.0
}

/// Whole note (4 beats).
pub const fn w() -> f32 {
    4.0
}

/// Eighth note (0.5 beats).
pub const fn e() -> f32 {
    0.5
}

/// Sixteenth note (0.25 beats).
pub const fn s() -> f32 {
    0.25
}

/// Thirty-second note (0.125 beats).
pub const fn t() -> f32 {
    0.125
}

/// Dotted duration (1.5× the original).
pub const fn dotted(d: f32) -> f32 {
    d * 1.5
}

/// Triplet duration (2/3 of the original).
pub const fn triplet(d: f32) -> f32 {
    d * 2.0 / 3.0
}

/// Convert bars to beats (4/4).
pub const fn bars(n: f32) -> f32 {
    n * 4.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_durations() {
        assert_eq!(q(), 1.0);
        assert_eq!(h(), 2.0);
        assert_eq!(w(), 4.0);
        assert_eq!(e(), 0.5);
        assert_eq!(s(), 0.25);
    }

    #[test]
    fn modifiers() {
        assert_eq!(dotted(q()), 1.5);
        assert!((triplet(q()) - 2.0 / 3.0).abs() < 0.001);
        assert_eq!(bars(2.0), 8.0);
    }
}
