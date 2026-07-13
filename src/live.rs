//! The scene runtime: the builder surface, hot-reload diffing, sample cache,
//! offline render, cpal setup, and MIDI. This is the only layer the user's
//! `scene` and track functions touch.

pub mod fx_builder;
pub mod render;
pub mod scene;

#[cfg(feature = "hot-reload")]
mod app;
#[cfg(feature = "midi")]
pub(crate) mod midi;

pub use render::render;
pub use scene::{GroupBuilder, OscBuilder, Scene, SceneTrack};

#[cfg(feature = "hot-reload")]
pub use app::live;
