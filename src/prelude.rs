//! One `use autosynth::prelude::*;` brings in the whole live-coding surface.

// ── Scene API (primary) ──
pub use crate::live::{GroupBuilder, OscBuilder, Scene, SceneTrack as Track};
pub use crate::live::render;
#[cfg(feature = "hot-reload")]
pub use crate::live::live;

// ── Timing / automation ──
pub use crate::model::param::IntoVal;
pub use crate::music::{Clock, Phrase};

// ── Composition enums used in builder calls ──
pub use crate::dsp::effects::{DelayMode, DistortionMode};
pub use crate::dsp::envelope::RetriggerMode;
pub use crate::dsp::filter::FilterType;
pub use crate::dsp::oscillator::Oscillator;
pub use crate::dsp::waveform::Waveform;

// ── Durations (one name per concept) ──
pub use crate::music::duration::{bars, dotted, e, h, q, s, t, triplet, w};

// ── Patterns ──
pub use crate::music::pattern::{Pattern, arp, euclidean};

// ── Harmony: scales, chords, and functions ──
pub use crate::music::harmony::*;

// ── Note constants and `note("C#4")` ──
pub use crate::music::notes::*;
