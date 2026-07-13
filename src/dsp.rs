//! Pure DSP building blocks: oscillators, envelope, filter, and effects.
//! Nothing here knows about tracks, scheduling, or the scene — these are the
//! sample-crunching primitives the engine wires together.

pub mod effects;
pub mod envelope;
pub mod filter;
pub mod oscillator;
pub mod smooth;
pub mod waveform;

pub use filter::{FilterType, Svf};
pub use waveform::Waveform;

/// A stereo sample pair `[left, right]`.
pub type StereoFrame = [f32; 2];

/// Convert a (possibly fractional) MIDI note number to frequency in Hz.
pub fn midi_to_freq(note: f32) -> f32 {
    440.0 * 2.0_f32.powf((note - 69.0) / 12.0)
}
