// Scale intervals
pub const MAJOR: &[u8] = &[0, 2, 4, 5, 7, 9, 11];
pub const MINOR: &[u8] = &[0, 2, 3, 5, 7, 8, 10];
pub const DORIAN: &[u8] = &[0, 2, 3, 5, 7, 9, 10];
pub const PHRYGIAN: &[u8] = &[0, 1, 3, 5, 7, 8, 10];
pub const LYDIAN: &[u8] = &[0, 2, 4, 6, 7, 9, 11];
pub const MIXOLYDIAN: &[u8] = &[0, 2, 4, 5, 7, 9, 10];
pub const LOCRIAN: &[u8] = &[0, 1, 3, 5, 6, 8, 10];
pub const HARMONIC_MINOR: &[u8] = &[0, 2, 3, 5, 7, 8, 11];
pub const MELODIC_MINOR: &[u8] = &[0, 2, 3, 5, 7, 9, 11];
pub const PENTATONIC: &[u8] = &[0, 2, 4, 7, 9];
pub const PENTATONIC_MINOR: &[u8] = &[0, 3, 5, 7, 10];
pub const BLUES: &[u8] = &[0, 3, 5, 6, 7, 10];
pub const WHOLE_TONE: &[u8] = &[0, 2, 4, 6, 8, 10];
pub const CHROMATIC: &[u8] = &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];

// Chord intervals
pub const MAJ: &[u8] = &[0, 4, 7];
pub const MIN: &[u8] = &[0, 3, 7];
pub const DIM: &[u8] = &[0, 3, 6];
pub const AUG: &[u8] = &[0, 4, 8];
pub const SUS2: &[u8] = &[0, 2, 7];
pub const SUS4: &[u8] = &[0, 5, 7];
pub const MAJ7: &[u8] = &[0, 4, 7, 11];
pub const MIN7: &[u8] = &[0, 3, 7, 10];
pub const DOM7: &[u8] = &[0, 4, 7, 10];
pub const DIM7: &[u8] = &[0, 3, 6, 9];
pub const POWER: &[u8] = &[0, 7];

/// A key: a root note plus a scale. First-class value so call sites state the
/// key once (`let key = Key::new(E4, MAJOR)`) and then ask for degrees, triads,
/// and sevenths without restating it.
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

    /// Diatonic triad rooted at degree `d`: `deg(d)`, `deg(d+2)`, `deg(d+4)`.
    pub fn triad(&self, d: i32) -> [u8; 3] {
        [self.deg(d), self.deg(d + 2), self.deg(d + 4)]
    }

    /// Diatonic seventh chord rooted at degree `d`.
    pub fn seventh(&self, d: i32) -> [u8; 4] {
        [self.deg(d), self.deg(d + 2), self.deg(d + 4), self.deg(d + 6)]
    }
}

pub fn chord(root: u8, intervals: &[u8]) -> Vec<u8> {
    intervals.iter().map(|&i| root + i).collect()
}

pub fn scale(root: u8, intervals: &[u8]) -> Vec<u8> {
    intervals.iter().map(|&i| root + i).collect()
}

pub fn invert(notes: &[u8], n: usize) -> Vec<u8> {
    if notes.is_empty() || n == 0 {
        return notes.to_vec();
    }
    let mut result = notes.to_vec();
    for _ in 0..n {
        if let Some(&first) = result.first() {
            result.remove(0);
            result.push(first + 12);
        }
    }
    result
}

pub fn transpose(notes: &[u8], semitones: i8) -> Vec<u8> {
    notes
        .iter()
        .map(|&n| (n as i16 + semitones as i16).clamp(0, 127) as u8)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::music::notes::{C4, C5, E4, G4};

    #[test]
    fn deg_is_one_indexed_and_total() {
        let key = Key::new(C4, MAJOR);
        assert_eq!(key.deg(1), C4, "degree 1 is the root");
        assert_eq!(key.deg(3), E4);
        assert_eq!(key.deg(5), G4);
        assert_eq!(key.deg(8), C5, "degree 8 is the octave");
        // Degree 0 and negatives fall below the root (no deg(0) == deg(1) bug).
        assert_eq!(key.deg(0), C4 - 1, "deg(0) is a semitone below the root (B3)");
        assert_eq!(key.deg(-6), C4 - 12, "one octave below the root");
    }

    #[test]
    fn triad_and_seventh() {
        let key = Key::new(C4, MAJOR);
        assert_eq!(key.triad(1), [C4, E4, G4]);
        assert_eq!(key.seventh(1), [C4, E4, G4, C4 + 11]);
    }
}
