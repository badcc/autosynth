pub mod chorus;
pub mod delay;
pub mod distortion;

pub use chorus::Chorus;
pub use delay::{Delay, DelayMode};
pub use distortion::{Distortion, DistortionMode};

pub type StereoFrame = [f32; 2];

pub trait Effect: Send {
    /// Process a stereo frame. Called per-sample in the render loop.
    fn process(&mut self, frame: StereoFrame, sample_rate: f32) -> StereoFrame;

    /// Reset internal state (e.g., on track stop).
    fn reset(&mut self);

    /// Set a numbered parameter slot to a value. Default no-op.
    fn set_param(&mut self, _slot: u8, _value: f32) {}
}
