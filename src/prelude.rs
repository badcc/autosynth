//! One `use autosynth::prelude::*;` brings in the whole live-coding surface.

// ── Scene, builders ──
pub use crate::live::chain_builder::Chain;
pub use crate::live::scene::Scene;
pub use crate::live::track_builder::{Bus, Track};
pub use crate::model::chain::CustomEffect;
pub use crate::model::instrument::{Sampler, Synth};

// ── Libraries ──
pub use crate::{chains, presets};

// ── Signals and form ──
pub use crate::music::form::Section;
pub use crate::music::signal::{
    Ctx, Mod, Signal, adsr, after, curve, during, env, key, knob, lfo, noise, rnd, saw, sine, smooth_noise, square,
    tri, vel,
};

// ── Patterns ──
pub use crate::music::phrase::{Arp, Phrase};

// ── Enums used in builder calls: `Saw`, `Hard`, `Arp::UpDown` ──
pub use crate::dsp::envelope::RetriggerMode::{self, *};
pub use crate::dsp::waveform::Waveform::{self, *};

// ── Durations, harmony, notes ──
pub use crate::music::duration::{DurExt, N1, N2, N4, N8, N16, N32, bars};
pub use crate::music::harmony::*;
pub use crate::music::notes::*;
pub use crate::music::pitch::Key;
