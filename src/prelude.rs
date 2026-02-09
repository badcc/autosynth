// ── Scene API (primary) ──
pub use crate::automation::{Clock, IntoVal, Phrase};
pub use crate::live::live;
pub use crate::scene::{OscBuilder, Scene, SceneTrack as Track};

// ── Composition ──
pub use crate::envelope::RetriggerMode;
pub use crate::filter::FilterType;
pub use crate::oscillator::Oscillator;
pub use crate::patch::Patch;
pub use crate::pattern::{arp, euclidean, seq, Pattern};
pub use crate::score::{b, bar, Tempo, Time};
pub use crate::waveform::Waveform;

// ── Effects ──
pub use crate::effects::{DelayMode, DistortionMode};

// ── Duration helpers ──
pub use crate::duration::{bars, dot, dotted, e, h, q, s, t, triplet, w};

// ── Harmony ──
pub use crate::harmony::{chord, degree, diatonic_triad, invert, scale, transpose};
pub use crate::harmony::{
    AUG, BLUES, CHROMATIC, DIM, DIM7, DOM7, DORIAN, HARMONIC_MINOR, LOCRIAN, LYDIAN, MAJ, MAJ7,
    MAJOR, MELODIC_MINOR, MIN, MIN7, MINOR, MIXOLYDIAN, PENTATONIC, PENTATONIC_MINOR, PHRYGIAN,
    POWER, SUS2, SUS4, WHOLE_TONE,
};

// ── Notes ──
pub use crate::notes::*;
