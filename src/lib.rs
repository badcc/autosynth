//! autosynth — a Rust live-coding synthesizer where plain functions *are* the
//! music. Five strictly-ordered layers, each depending only on those above:
//!
//! - [`music`]  pure vocabulary: notes, keys, form, signals, mini-notation, `Phrase`
//! - [`dsp`]    pure processors: oscillators, envelopes, filters, effects
//! - [`model`]  declarative, diffable scene description
//! - [`engine`] real-time: transport, scheduler, voices, chains, buses
//! - [`live`]   scene runtime: builders, hot-reload diffing, samples, MIDI, cpal
//!
//! Plus two libraries of plain functions: [`presets`] (instruments) and
//! [`chains`] (effect chains). `use autosynth::prelude::*;` brings in the
//! whole live-coding surface.

pub mod chains;
pub mod dsp;
pub mod engine;
pub mod live;
pub mod model;
pub mod music;
pub mod prelude;
pub mod presets;
pub mod sample;

#[cfg(feature = "hot-reload")]
pub use live::app::live;
pub use live::render::render;
