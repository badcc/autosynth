//! autosynth — a Rust live-coding synthesizer where plain functions *are* the
//! music. Five strictly-ordered layers, each depending only on those above it:
//!
//! - [`music`]  pure vocabulary: notes, harmony, durations, `Phrase`, `Clock`
//! - [`model`]  declarative, diffable scene description
//! - [`dsp`]    pure per-block processors: oscillators, ADSR, SVF, effects
//! - [`engine`] real-time: transport, scheduler, voice banks, mixer, commands
//! - [`live`]   scene runtime: hot-reload diffing, sample cache, MIDI, cpal

pub mod dsp;
pub mod engine;
pub mod live;
pub mod model;
pub mod music;
pub mod prelude;
pub mod sample;

pub use live::render;
#[cfg(feature = "hot-reload")]
pub use live::live;
