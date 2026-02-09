// ── Scene API (primary) ──
pub use crate::live::live;
pub use crate::scene::{Scene, SceneTrack as Track};

// ── Composition ──
pub use crate::envelope::RetriggerMode;
pub use crate::filter::FilterType;
pub use crate::oscillator::Oscillator;
pub use crate::patch::Patch;
pub use crate::pattern::{arp, euclidean, seq, Pattern};
pub use crate::score::{b, bar, Score, Tempo, Time};
pub use crate::waveform::Waveform;

// ── Effects (for direct use / old API) ──
pub use crate::effects::{Chorus, Delay, DelayMode, Distortion, DistortionMode, Effect, StereoFrame};

// ── Events ──
pub use crate::event::{EventKind, Param};

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

// ── Advanced / direct use (old API still accessible) ──
pub use crate::clip::Clip;
pub use crate::engine::{Engine, EngineHandle};
pub use crate::session::Session;
pub use crate::synth::Synth;
