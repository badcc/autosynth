//! Scales, chords, roman numerals and voice leading.

use crate::music::pitch::Key;

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
pub const HALF_DIM7: &[u8] = &[0, 3, 6, 10];
pub const MIN_MAJ7: &[u8] = &[0, 3, 7, 11];
pub const ADD9: &[u8] = &[0, 4, 7, 14];
pub const MIN_ADD9: &[u8] = &[0, 3, 7, 14];
pub const POWER: &[u8] = &[0, 7];

/// Notes of a chord built on `root`.
pub fn chord(root: u8, intervals: &[u8]) -> Vec<u8> {
    intervals.iter().map(|&i| root.saturating_add(i).min(127)).collect()
}

/// Rotate the lowest `n` notes up an octave.
pub fn invert(notes: &[u8], n: usize) -> Vec<u8> {
    let mut v = notes.to_vec();
    v.sort_unstable();
    for _ in 0..n.min(v.len()) {
        let low = v.remove(0);
        v.push(low.saturating_add(12).min(127));
    }
    v
}

/// Raise every other note (2nd, 4th, …) by `octaves` — an open, spread voicing.
pub fn spread(notes: &[u8], octaves: u8) -> Vec<u8> {
    let mut v = notes.to_vec();
    v.sort_unstable();
    for (i, n) in v.iter_mut().enumerate() {
        if i % 2 == 1 {
            *n = n.saturating_add(12 * octaves).min(127);
        }
    }
    v.sort_unstable();
    v
}

/// Re-voice `notes` (same pitch classes) to move as little as possible from
/// `prev`: every inversion in three octave placements is tried, and the one
/// with the least total movement against `prev`'s nearest notes wins.
pub fn voice_lead(prev: &[u8], notes: &[u8]) -> Vec<u8> {
    if prev.is_empty() || notes.is_empty() {
        return notes.to_vec();
    }
    let mut base = notes.to_vec();
    base.sort_unstable();
    let mut best = base.clone();
    let mut best_cost = i32::MAX;
    for inv in 0..base.len() {
        let voiced = invert(&base, inv);
        for shift in [-24i32, -12, 0, 12] {
            let cand: Vec<i32> = voiced.iter().map(|&n| n as i32 + shift).collect();
            if cand.iter().any(|&n| !(0..=127).contains(&n)) {
                continue;
            }
            let cost: i32 = cand
                .iter()
                .map(|&n| prev.iter().map(|&p| (n - p as i32).abs()).min().unwrap_or(0))
                .sum::<i32>()
                + prev
                    .iter()
                    .map(|&p| cand.iter().map(|&n| (n - p as i32).abs()).min().unwrap_or(0))
                    .sum::<i32>();
            if cost < best_cost {
                best_cost = cost;
                best = cand.iter().map(|&n| n as u8).collect();
            }
        }
    }
    best
}

impl Key {
    /// Parse a roman-numeral chord in this key: the numeral picks the scale
    /// degree for the root and its case the quality (`IV` major, `ii` minor).
    ///
    /// Suffixes: `°`/`dim`, `+`/`aug`, `ø` (half-diminished 7th), `7`
    /// (dominant 7th on upper case, minor 7th on lower), `maj7`, `sus2`,
    /// `sus4`, `add9`. A leading `b`/`#` alters the root; a trailing `/n`
    /// picks the n-th inversion. Examples: `"i"`, `"VI"`, `"V7"`, `"ii7/1"`,
    /// `"bVII"`, `"vii°"`.
    pub fn roman(&self, s: &str) -> Option<Vec<u8>> {
        let (body, inversion) = match s.split_once('/') {
            Some((b, i)) => (b, i.parse::<usize>().ok()?),
            None => (s, 0),
        };
        let mut rest = body;
        let mut accidental = 0i32;
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
        let numeral_len = rest.chars().take_while(|c| matches!(c, 'i' | 'v' | 'I' | 'V')).count();
        if numeral_len == 0 {
            return None;
        }
        let numeral = &rest[..numeral_len];
        let suffix = &rest[numeral_len..];
        let degree = match numeral.to_ascii_lowercase().as_str() {
            "i" => 1,
            "ii" => 2,
            "iii" => 3,
            "iv" => 4,
            "v" => 5,
            "vi" => 6,
            "vii" => 7,
            _ => return None,
        };
        let upper = numeral.chars().next()?.is_ascii_uppercase();
        let intervals = match (suffix, upper) {
            ("", true) => MAJ,
            ("", false) => MIN,
            ("°" | "dim" | "o", _) => DIM,
            ("°7" | "dim7" | "o7", _) => DIM7,
            ("ø" | "ø7", _) => HALF_DIM7,
            ("+" | "aug", _) => AUG,
            ("7", true) => DOM7,
            ("7", false) => MIN7,
            ("maj7" | "M7", true) => MAJ7,
            ("maj7" | "M7", false) => MIN_MAJ7,
            ("sus2", _) => SUS2,
            ("sus4" | "sus", _) => SUS4,
            ("add9", true) => ADD9,
            ("add9", false) => MIN_ADD9,
            _ => return None,
        };
        let root = (self.deg(degree) as i32 + accidental).clamp(0, 127) as u8;
        Some(invert(&chord(root, intervals), inversion))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::music::notes::*;

    #[test]
    fn roman_numerals_follow_case_and_key() {
        let am = Key::new(A2, MINOR);
        assert_eq!(am.roman("i").unwrap(), vec![A2, C3, E3]);
        assert_eq!(am.roman("VI").unwrap(), vec![F3, A3, C4]);
        assert_eq!(am.roman("III").unwrap(), vec![C3, E3, G3]);
        assert_eq!(am.roman("VII").unwrap(), vec![G3, B3, D4]);
        let c = Key::new(C4, MAJOR);
        assert_eq!(c.roman("V7").unwrap(), vec![G4, B4, D5, F5]);
        assert_eq!(c.roman("ii7").unwrap(), vec![D4, F4, A4, C5]);
        assert_eq!(c.roman("vii°").unwrap(), vec![B4, D5, F5]);
        assert_eq!(c.roman("bVII").unwrap(), vec![AS4, D5, F5]);
        assert_eq!(c.roman("I/1").unwrap(), vec![E4, G4, C5]);
        assert!(c.roman("X").is_none());
        assert!(c.roman("Iwat").is_none());
    }

    #[test]
    fn voice_leading_minimizes_movement() {
        let c = Key::new(C4, MAJOR);
        let one = c.roman("I").unwrap(); // C E G
        let four = c.roman("IV").unwrap(); // F A C
        let led = voice_lead(&one, &four);
        // Closest voicing keeps C and moves E→F, G→A: C4 F4 A4.
        assert_eq!(led, vec![C4, F4, A4]);
    }

    #[test]
    fn spread_opens_the_voicing() {
        assert_eq!(spread(&[C4, E4, G4], 1), vec![C4, G4, E5]);
    }
}
