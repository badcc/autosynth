//! Keys and scale degrees. A [`Key`] is the harmonic context patterns resolve
//! against: set it once per scene (`s.key(A2, MINOR)`), and degrees like `"1"`,
//! `"b3"` or `"8"` in pattern strings become notes late, at phrase time.

use crate::music::notes::note;

/// A root note plus a scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Key {
    pub root: u8,
    pub scale: &'static [u8],
}

impl Key {
    pub const fn new(root: u8, scale: &'static [u8]) -> Self {
        Self { root, scale }
    }

    /// The note at scale degree `d`, 1-indexed and total over all of `i32`:
    /// `deg(1)` is the root, `deg(8)` is the octave, and `deg(0)` or negatives
    /// wrap below the root. No silent `deg(0) == deg(1)` footgun.
    pub fn deg(&self, d: i32) -> u8 {
        let len = self.scale.len() as i32;
        let i = d - 1;
        let step = self.scale[i.rem_euclid(len) as usize] as i32;
        let octave = i.div_euclid(len) * 12;
        (self.root as i32 + step + octave).clamp(0, 127) as u8
    }

    /// Diatonic triad rooted at degree `d`.
    pub fn triad(&self, d: i32) -> [u8; 3] {
        [self.deg(d), self.deg(d + 2), self.deg(d + 4)]
    }

    /// Diatonic seventh chord rooted at degree `d`.
    pub fn seventh(&self, d: i32) -> [u8; 4] {
        [self.deg(d), self.deg(d + 2), self.deg(d + 4), self.deg(d + 6)]
    }

    /// The same scale, `semis` semitones higher.
    pub fn transpose(self, semis: i32) -> Key {
        Key { root: (self.root as i32 + semis).clamp(0, 127) as u8, scale: self.scale }
    }

    /// Resolve a pattern atom to a note, with degrees offset by `shift` scale
    /// steps. Atoms are either scale degrees — `1`, `8`, `-2`, with any number
    /// of leading `b`/`#` accidentals (`b3`, `#4`) — or note names starting with
    /// an uppercase letter (`C4`, `Eb2`, `F#3`).
    pub fn resolve(&self, atom: &str, shift: i32) -> Option<u8> {
        let first = atom.chars().next()?;
        if first.is_ascii_uppercase() {
            return note(atom);
        }
        let mut accidental = 0i32;
        let mut rest = atom;
        loop {
            if let Some(r) = rest.strip_prefix('b') {
                accidental -= 1;
                rest = r;
            } else if let Some(r) = rest.strip_prefix('#') {
                accidental += 1;
                rest = r;
            } else {
                break;
            }
        }
        let d: i32 = rest.parse().ok()?;
        Some((self.deg(d + shift) as i32 + accidental).clamp(0, 127) as u8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::music::harmony::{MAJOR, MINOR};
    use crate::music::notes::{A2, C4, C5, E4, G4};

    #[test]
    fn deg_is_one_indexed_and_total() {
        let key = Key::new(C4, MAJOR);
        assert_eq!(key.deg(1), C4, "degree 1 is the root");
        assert_eq!(key.deg(3), E4);
        assert_eq!(key.deg(5), G4);
        assert_eq!(key.deg(8), C5, "degree 8 is the octave");
        assert_eq!(key.deg(0), C4 - 1, "deg(0) is a step below the root (B3)");
        assert_eq!(key.deg(-6), C4 - 12, "one octave below the root");
    }

    #[test]
    fn triad_and_seventh() {
        let key = Key::new(C4, MAJOR);
        assert_eq!(key.triad(1), [C4, E4, G4]);
        assert_eq!(key.seventh(1), [C4, E4, G4, C4 + 11]);
    }

    #[test]
    fn resolve_degrees_accidentals_and_names() {
        let key = Key::new(C4, MAJOR);
        assert_eq!(key.resolve("1", 0), Some(C4));
        assert_eq!(key.resolve("b3", 0), Some(E4 - 1));
        assert_eq!(key.resolve("#4", 0), Some(C4 + 6));
        assert_eq!(key.resolve("-6", 0), Some(C4 - 12));
        assert_eq!(key.resolve("1", 4), Some(G4), "shifted to degree 5");
        assert_eq!(key.resolve("B3", 0), Some(59), "uppercase is a note name");
        assert_eq!(key.resolve("zz", 0), None);
        let am = Key::new(A2, MINOR);
        assert_eq!(am.resolve("3", 0), Some(A2 + 3));
    }
}
