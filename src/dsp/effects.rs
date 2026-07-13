//! Stereo effects. Each effect is a pure per-frame processor with numbered
//! parameter slots (so automation can drive any of them by index). The
//! declarative param list and diffable config live in `model::fx`.

pub mod chorus;
pub mod delay;
pub mod distortion;
pub mod reverb;

pub use chorus::Chorus;
pub use delay::{Delay, DelayMode};
pub use distortion::{Distortion, DistortionMode};
pub use reverb::Reverb;

use crate::dsp::StereoFrame;

pub trait Effect: Send {
    /// Process one stereo frame.
    fn process(&mut self, frame: StereoFrame, sample_rate: f32) -> StereoFrame;

    /// Reset internal state (buffers, filters).
    fn reset(&mut self);

    /// Set a numbered parameter slot. Slots are defined per effect in `model::fx`.
    fn set_param(&mut self, _slot: u8, _value: f32) {}
}
