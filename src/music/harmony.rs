// Scale intervals
pub const MAJOR: [u8; 7] = [0, 2, 4, 5, 7, 9, 11];
pub const MINOR: [u8; 7] = [0, 2, 3, 5, 7, 8, 10];
pub const DORIAN: [u8; 7] = [0, 2, 3, 5, 7, 9, 10];
pub const PHRYGIAN: [u8; 7] = [0, 1, 3, 5, 7, 8, 10];
pub const LYDIAN: [u8; 7] = [0, 2, 4, 6, 7, 9, 11];
pub const MIXOLYDIAN: [u8; 7] = [0, 2, 4, 5, 7, 9, 10];
pub const LOCRIAN: [u8; 7] = [0, 1, 3, 5, 6, 8, 10];
pub const HARMONIC_MINOR: [u8; 7] = [0, 2, 3, 5, 7, 8, 11];
pub const MELODIC_MINOR: [u8; 7] = [0, 2, 3, 5, 7, 9, 11];
pub const PENTATONIC: [u8; 5] = [0, 2, 4, 7, 9];
pub const PENTATONIC_MINOR: [u8; 5] = [0, 3, 5, 7, 10];
pub const BLUES: [u8; 6] = [0, 3, 5, 6, 7, 10];
pub const WHOLE_TONE: [u8; 6] = [0, 2, 4, 6, 8, 10];
pub const CHROMATIC: [u8; 12] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];

// Chord intervals
pub const MAJ: [u8; 3] = [0, 4, 7];
pub const MIN: [u8; 3] = [0, 3, 7];
pub const DIM: [u8; 3] = [0, 3, 6];
pub const AUG: [u8; 3] = [0, 4, 8];
pub const SUS2: [u8; 3] = [0, 2, 7];
pub const SUS4: [u8; 3] = [0, 5, 7];
pub const MAJ7: [u8; 4] = [0, 4, 7, 11];
pub const MIN7: [u8; 4] = [0, 3, 7, 10];
pub const DOM7: [u8; 4] = [0, 4, 7, 10];
pub const DIM7: [u8; 4] = [0, 3, 6, 9];
pub const POWER: [u8; 2] = [0, 7];

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

pub fn degree(root: u8, scale_intervals: &[u8], deg: usize) -> u8 {
    if deg == 0 || scale_intervals.is_empty() {
        return root;
    }
    let idx = (deg - 1) % scale_intervals.len();
    let octaves = ((deg - 1) / scale_intervals.len()) as u8;
    root + scale_intervals[idx] + octaves * 12
}

pub fn diatonic_triad(root: u8, scale_intervals: &[u8], deg: usize) -> Vec<u8> {
    vec![
        degree(root, scale_intervals, deg),
        degree(root, scale_intervals, deg + 2),
        degree(root, scale_intervals, deg + 4),
    ]
}
