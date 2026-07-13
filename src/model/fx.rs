//! Declarative effect descriptions. Each effect is described *once* here: a
//! diffable config struct, its parameter-slot constants, and a `build` that
//! constructs the DSP processor. Adding an effect means writing its DSP (in
//! `dsp::effects`) and its config here — nothing else.

use crate::dsp::effects::{Chorus, Delay, DelayMode, Distortion, DistortionMode, Effect, Reverb};

/// One effect in a track's chain: its parameters plus an enable flag. Because
/// `enabled` lives inside the diffable spec, toggling it is a visible change on
/// hot-reload (unlike the old out-of-band flag).
#[derive(Clone, Debug, PartialEq)]
pub struct FxSpec {
    pub kind: FxKind,
    pub enabled: bool,
}

impl FxSpec {
    pub fn build(&self, bpm: f32, sample_rate: f32) -> Box<dyn Effect> {
        self.kind.build(bpm, sample_rate)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum FxKind {
    Delay(DelayCfg),
    Chorus(ChorusCfg),
    Distortion(DistortionCfg),
    Reverb(ReverbCfg),
}

impl FxKind {
    pub fn build(&self, bpm: f32, sample_rate: f32) -> Box<dyn Effect> {
        match self {
            FxKind::Delay(c) => Box::new(c.build(bpm, sample_rate)),
            FxKind::Chorus(c) => Box::new(c.build(sample_rate)),
            FxKind::Distortion(c) => Box::new(c.build()),
            FxKind::Reverb(c) => Box::new(c.build(sample_rate)),
        }
    }
}

// ── Delay ──

pub mod delay_slots {
    pub const PARAM_FEEDBACK: u8 = 0;
    pub const PARAM_MIX: u8 = 1;
}

#[derive(Clone, Debug, PartialEq)]
pub struct DelayCfg {
    pub beats: Option<f32>,
    pub seconds: Option<f32>,
    pub feedback: f32,
    pub mix: f32,
    pub mode: DelayMode,
}

impl Default for DelayCfg {
    fn default() -> Self {
        Self {
            beats: None,
            seconds: None,
            feedback: 0.3,
            mix: 0.3,
            mode: DelayMode::Normal,
        }
    }
}

impl DelayCfg {
    fn build(&self, bpm: f32, sample_rate: f32) -> Delay {
        let delay = if let Some(beats) = self.beats {
            Delay::tempo_synced(beats, bpm, self.feedback, self.mix, sample_rate)
        } else if let Some(seconds) = self.seconds {
            Delay::new(seconds, self.feedback, self.mix, sample_rate)
        } else {
            Delay::new(0.25, self.feedback, self.mix, sample_rate)
        };
        delay.mode(self.mode)
    }
}

// ── Chorus ──

pub mod chorus_slots {
    pub const PARAM_RATE: u8 = 0;
    pub const PARAM_DEPTH: u8 = 1;
    pub const PARAM_MIX: u8 = 2;
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChorusCfg {
    pub rate: f32,
    pub depth: f32,
    pub mix: f32,
}

impl Default for ChorusCfg {
    fn default() -> Self {
        Self {
            rate: 1.0,
            depth: 0.003,
            mix: 0.3,
        }
    }
}

impl ChorusCfg {
    fn build(&self, sample_rate: f32) -> Chorus {
        Chorus::new(self.rate, self.depth, self.mix, sample_rate)
    }
}

// ── Distortion ──

pub mod distortion_slots {
    pub const PARAM_DRIVE: u8 = 0;
    pub const PARAM_MIX: u8 = 1;
    pub const PARAM_BIAS: u8 = 2;
    pub const PARAM_TONE: u8 = 3;
    pub const PARAM_OUTPUT: u8 = 4;
}

#[derive(Clone, Debug, PartialEq)]
pub struct DistortionCfg {
    pub drive: f32,
    pub mix: f32,
    pub mode: DistortionMode,
    pub bias: f32,
    pub tone: f32,
    pub output_gain: f32,
}

impl Default for DistortionCfg {
    fn default() -> Self {
        Self {
            drive: 1.0,
            mix: 1.0,
            mode: DistortionMode::SoftClip,
            bias: 0.0,
            tone: 1.0,
            output_gain: 1.0,
        }
    }
}

impl DistortionCfg {
    fn build(&self) -> Distortion {
        Distortion::new(self.drive, self.mix)
            .mode(self.mode)
            .bias(self.bias)
            .tone(self.tone)
            .output_gain(self.output_gain)
    }
}

// ── Reverb ──

pub mod reverb_slots {
    pub const PARAM_SIZE: u8 = 0;
    pub const PARAM_DAMP: u8 = 1;
    pub const PARAM_MIX: u8 = 2;
    pub const PARAM_WIDTH: u8 = 3;
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReverbCfg {
    pub size: f32,
    pub damp: f32,
    pub mix: f32,
    pub width: f32,
}

impl Default for ReverbCfg {
    fn default() -> Self {
        Self {
            size: 0.5,
            damp: 0.5,
            mix: 0.3,
            width: 1.0,
        }
    }
}

impl ReverbCfg {
    fn build(&self, sample_rate: f32) -> Reverb {
        Reverb::new(self.size, self.damp, self.mix, self.width, sample_rate)
    }
}
