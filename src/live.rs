//! The scene runtime: builders, hot-reload diffing, sample cache, offline
//! render, cpal setup and MIDI. This is the only layer your `scene`, track, bus
//! and chain functions touch.

pub mod chain_builder;
pub mod render;
pub mod scene;
pub mod track_builder;

#[cfg(feature = "hot-reload")]
pub mod app;
#[cfg(feature = "midi")]
pub(crate) mod midi;
