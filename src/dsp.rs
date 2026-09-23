//! Pure DSP building blocks: oscillators, envelopes, filters and effects.
//! Nothing here knows about tracks, scheduling, or the scene — these are the
//! sample-crunching primitives the engine wires together.

pub mod effects;
pub mod envelope;
pub mod filter;
pub mod oscillator;
pub mod smooth;
pub mod waveform;

/// A stereo sample pair `[left, right]`.
pub type StereoFrame = [f32; 2];

/// Convert a (possibly fractional) MIDI note number to frequency in Hz.
pub fn midi_to_freq(note: f32) -> f32 {
    440.0 * 2.0_f32.powf((note - 69.0) / 12.0)
}

/// Decibels to linear gain.
pub fn db_to_gain(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

/// Linear gain to decibels (floored at -120 dB).
pub fn gain_to_db(g: f32) -> f32 {
    20.0 * g.max(1e-6).log10()
}
