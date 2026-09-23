//! Song form: named spans of bars, declared as `const`s and chained.
//!
//! ```ignore
//! const INTRO: Section = Section::start(16);
//! const BUILD: Section = INTRO.then(16);
//! const DROP:  Section = BUILD.then(32);
//! ```
//!
//! Sections are plain values: signals (`curve()`, `after`, `during`), phrases
//! (`p.during(DROP)`) and the transport (`s.jump(DROP)`, `s.hold(BUILD)`) all
//! read them. Bars are 4/4.

use crate::music::signal::{Ease, Expr, Seg, Signal};

/// A span of `bars` bars starting at bar `first` (0-based).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Section {
    pub first: u32,
    pub bars: u32,
}

impl Section {
    /// The opening section of a song: bars `0..bars`.
    pub const fn start(bars: u32) -> Section {
        Section { first: 0, bars }
    }

    /// An explicit span: `bars` bars from bar `first`.
    pub const fn at(first: u32, bars: u32) -> Section {
        Section { first, bars }
    }

    /// The section that follows this one, `bars` long.
    pub const fn then(self, bars: u32) -> Section {
        Section { first: self.first + self.bars, bars }
    }

    /// The span from the start of `self` to the end of `other`.
    pub const fn to(self, other: Section) -> Section {
        Section { first: self.first, bars: other.first + other.bars - self.first }
    }

    /// The bar just past the end.
    pub const fn end(self) -> u32 {
        self.first + self.bars
    }

    pub fn start_beat(self) -> f64 {
        self.first as f64 * 4.0
    }

    pub fn end_beat(self) -> f64 {
        self.end() as f64 * 4.0
    }

    pub fn beats(self) -> f64 {
        self.bars as f64 * 4.0
    }

    pub fn contains_beat(self, beat: f64) -> bool {
        beat >= self.start_beat() && beat < self.end_beat()
    }

    /// `0..1` progress through the section at `beat` (clamped).
    pub fn progress(self, beat: f64) -> f32 {
        if self.bars == 0 {
            return if beat >= self.start_beat() { 1.0 } else { 0.0 };
        }
        ((beat - self.start_beat()) / self.beats()).clamp(0.0, 1.0) as f32
    }

    /// A signal that moves `from → to` across this section, holding `from`
    /// before it and `to` after it.
    pub fn ramp(self, from: f32, to: f32) -> Signal {
        Signal(Expr::Curve(std::sync::Arc::from([Seg {
            start: self.start_beat(),
            len: self.beats(),
            from,
            to,
            ease: Ease::Linear,
        }])))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INTRO: Section = Section::start(16);
    const BUILD: Section = INTRO.then(8);
    const DROP: Section = BUILD.then(32);

    #[test]
    fn sections_chain() {
        assert_eq!(BUILD, Section { first: 16, bars: 8 });
        assert_eq!(DROP.first, 24);
        assert_eq!(INTRO.to(BUILD), Section { first: 0, bars: 24 });
        assert_eq!(BUILD.start_beat(), 64.0);
        assert_eq!(BUILD.end_beat(), 96.0);
    }

    #[test]
    fn contains_and_progress() {
        assert!(BUILD.contains_beat(64.0));
        assert!(!BUILD.contains_beat(96.0));
        assert_eq!(BUILD.progress(0.0), 0.0);
        assert_eq!(BUILD.progress(80.0), 0.5);
        assert_eq!(BUILD.progress(500.0), 1.0);
    }
}
