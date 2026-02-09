pub mod chorus;
pub mod delay;
pub mod distortion;

pub use chorus::Chorus;
pub use delay::Delay;
pub use distortion::Distortion;

pub trait Effect: Send {
    /// Process a single sample (mono). Called per-sample in the render loop.
    fn process(&mut self, sample: f32, sample_rate: f32) -> f32;

    /// Reset internal state (e.g., on track stop).
    fn reset(&mut self);
}
