//! Stereo effects. Each effect is a block processor with numbered parameter
//! slots; its slot table (`PARAMS`) lives next to its DSP, so adding an effect
//! is one file here plus one line in `model::chain`.
//!
//! Effects process whole control blocks and see the tempo and song position
//! ([`FxCtx`]), so beat-aware effects (synced delay, trance gate) follow tempo
//! changes without being rebuilt.

pub mod chorus;
pub mod compressor;
pub mod crush;
pub mod delay;
pub mod distortion;
pub mod eq;
pub mod filter;
pub mod gate;
pub mod limiter;
pub mod phaser;
pub mod reverb;
pub mod width;

use crate::dsp::StereoFrame;

/// What an effect may know about the moment it is processing.
#[derive(Clone, Copy, Debug)]
pub struct FxCtx {
    pub sample_rate: f32,
    pub bpm: f32,
    /// Song beat at the start of the block.
    pub beat: f64,
    pub beats_per_sample: f64,
}

/// One automatable parameter: its builder name and default value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParamDef {
    pub name: &'static str,
    pub default: f32,
}

pub trait Effect: Send {
    /// Process a block in place.
    fn process(&mut self, buf: &mut [StereoFrame], ctx: &FxCtx);

    /// Set parameter `slot` (an index into the effect's `PARAMS`).
    fn set(&mut self, slot: usize, value: f32);

    /// Clear internal state (buffers, filters).
    fn reset(&mut self) {}
}

/// Dry/wet crossfade.
#[inline]
pub(crate) fn mix(dry: f32, wet: f32, amount: f32) -> f32 {
    dry + (wet - dry) * amount
}
