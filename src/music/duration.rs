//! Duration constants and modifiers. All durations are in beats (a quarter note
//! = `1.0`). Named by their note fraction: `N4` is a quarter, `N8` an eighth.

/// Whole note (4 beats).
pub const N1: f32 = 4.0;
/// Half note (2 beats).
pub const N2: f32 = 2.0;
/// Quarter note (1 beat).
pub const N4: f32 = 1.0;
/// Eighth note (0.5 beats).
pub const N8: f32 = 0.5;
/// Sixteenth note (0.25 beats).
pub const N16: f32 = 0.25;
/// Thirty-second note (0.125 beats).
pub const N32: f32 = 0.125;

/// Duration modifiers, so `N8.dotted()` reads as "a dotted eighth".
pub trait DurExt {
    /// Dotted: 1.5× the duration.
    fn dotted(self) -> f32;
    /// Triplet: 2/3 of the duration.
    fn triplet(self) -> f32;
}

impl DurExt for f32 {
    fn dotted(self) -> f32 {
        self * 1.5
    }
    fn triplet(self) -> f32 {
        self * 2.0 / 3.0
    }
}

/// Convert bars to beats (4/4).
pub const fn bars(n: f32) -> f32 {
    n * 4.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fraction_consts() {
        assert_eq!(N1, 4.0);
        assert_eq!(N2, 2.0);
        assert_eq!(N4, 1.0);
        assert_eq!(N8, 0.5);
        assert_eq!(N16, 0.25);
        assert_eq!(N32, 0.125);
    }

    #[test]
    fn modifiers() {
        assert_eq!(N4.dotted(), 1.5);
        assert_eq!(N8.dotted(), 0.75);
        assert!((N4.triplet() - 2.0 / 3.0).abs() < 1e-6);
        assert_eq!(bars(2.0), 8.0);
    }
}
