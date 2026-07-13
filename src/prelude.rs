//! One `use autosynth::prelude::*;` brings in the whole live-coding surface.

// ── Scene API (primary) ──
pub use crate::live::{GroupBuilder, OscBuilder, Scene, SceneTrack as Track};
pub use crate::live::render;
#[cfg(feature = "hot-reload")]
pub use crate::live::live;

// ── Timing / automation ──
pub use crate::model::param::IntoVal;
pub use crate::music::{Clock, Phrase};

// ── Composition enums used in builder calls. The glob imports let you write
// `t.osc(Saw, 0.5)`, `t.filter(Notch)`, `t.retrigger(Legato)`. ──
pub use crate::dsp::effects::{DelayMode, DistortionMode};
pub use crate::dsp::envelope::RetriggerMode;
pub use crate::dsp::envelope::RetriggerMode::*;
pub use crate::dsp::filter::FilterType;
pub use crate::dsp::filter::FilterType::*;
pub use crate::dsp::oscillator::Oscillator;
pub use crate::dsp::waveform::Waveform;
pub use crate::dsp::waveform::Waveform::*;

// ── Durations: `N4`, `N8`, … and the `.dotted()`/`.triplet()` modifiers ──
pub use crate::music::duration::{DurExt, N1, N2, N4, N8, N16, N32, bars};

// ── Automation shapes ──
pub use crate::music::shape::{Shape, ramp, saw, sine, tri};

// ── Patterns ──
pub use crate::music::pattern::{Pattern, arp, euclidean};

// ── Harmony: `Key`, scales, chords, and the chord/scale functions ──
pub use crate::music::harmony::*;

// ── Note constants and `note("C#4")` ──
pub use crate::music::notes::*;
