/// Duration helpers for live-coding.
///
/// All durations are expressed in beats (quarter notes = 1.0 beat).
/// These work with any tempo - the synth converts beats to samples.

/// Quarter note (1 beat)
pub const fn q() -> f32 {
    1.0
}

/// Half note (2 beats)
pub const fn h() -> f32 {
    2.0
}

/// Whole note (4 beats)
pub const fn w() -> f32 {
    4.0
}

/// Eighth note (0.5 beats)
pub const fn e() -> f32 {
    0.5
}

/// Sixteenth note (0.25 beats)
pub const fn s() -> f32 {
    0.25
}

/// Thirty-second note (0.125 beats)
pub const fn t() -> f32 {
    0.125
}

/// Dotted duration (1.5x the original)
pub const fn dotted(d: f32) -> f32 {
    d * 1.5
}

/// Shorter alias for dotted
pub const fn dot(d: f32) -> f32 {
    dotted(d)
}

/// Triplet duration (2/3 of the original)
pub const fn triplet(d: f32) -> f32 {
    d * 2.0 / 3.0
}

/// Shorter alias for triplet
pub const fn tri(d: f32) -> f32 {
    triplet(d)
}

/// Convert bars to beats (assumes 4/4 time)
pub const fn bars(n: f32) -> f32 {
    n * 4.0
}

/// Convert bars to beats with custom beats per bar
pub const fn bars_of(n: f32, beats_per_bar: f32) -> f32 {
    n * beats_per_bar
}

/// Double-dotted duration (1.75x the original)
pub const fn double_dotted(d: f32) -> f32 {
    d * 1.75
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_durations() {
        assert_eq!(q(), 1.0);
        assert_eq!(h(), 2.0);
        assert_eq!(w(), 4.0);
        assert_eq!(e(), 0.5);
        assert_eq!(s(), 0.25);
    }

    #[test]
    fn test_modifiers() {
        assert_eq!(dotted(q()), 1.5);
        assert_eq!(dot(h()), 3.0);
        assert!((triplet(q()) - 2.0 / 3.0).abs() < 0.001);
    }

    #[test]
    fn test_bars() {
        assert_eq!(bars(1.0), 4.0);
        assert_eq!(bars(2.0), 8.0);
        assert_eq!(bars_of(1.0, 3.0), 3.0); // 3/4 time
    }
}
