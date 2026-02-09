use crate::automation::{Clock, IntoVal, Val};
use crate::effects::{Chorus, Delay, DelayMode, Distortion, DistortionMode, Effect};
use crate::effects::chorus as chorus_params;
use crate::effects::delay as delay_params;
use crate::effects::distortion as distortion_params;

// ── Configs (PartialEq for diffing) ──

#[derive(Clone, Debug, PartialEq)]
pub enum EffectConfig {
    Delay(DelayConfig),
    Distortion(DistortionConfig),
    Chorus(ChorusConfig),
}

impl EffectConfig {
    /// Build the actual Effect for the audio thread.
    pub fn build(&self, bpm: f32, sample_rate: f32) -> Box<dyn Effect> {
        match self {
            EffectConfig::Delay(c) => Box::new(c.build(bpm, sample_rate)),
            EffectConfig::Distortion(c) => Box::new(c.build()),
            EffectConfig::Chorus(c) => Box::new(c.build(sample_rate)),
        }
    }
}

// ── Delay ──

#[derive(Clone, Debug, PartialEq)]
pub struct DelayConfig {
    pub beats: Option<f32>,
    pub seconds: Option<f32>,
    pub feedback: f32,
    pub mix: f32,
    pub mode: DelayMode,
}

impl DelayConfig {
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

pub struct DelayBuilder {
    config: DelayConfig,
    automations: Vec<(u8, Box<dyn FnMut(Clock) -> f32 + Send>)>,
    enabled_auto: Option<Box<dyn FnMut(Clock) -> bool + Send>>,
    initial_enabled: bool,
}

impl Default for DelayBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl DelayBuilder {
    pub fn new() -> Self {
        Self {
            config: DelayConfig {
                beats: None,
                seconds: None,
                feedback: 0.3,
                mix: 0.3,
                mode: DelayMode::Normal,
            },
            automations: Vec::new(),
            enabled_auto: None,
            initial_enabled: true,
        }
    }

    pub fn beats(&mut self, b: f32) -> &mut Self {
        self.config.beats = Some(b);
        self
    }

    pub fn seconds(&mut self, s: f32) -> &mut Self {
        self.config.seconds = Some(s);
        self
    }

    pub fn feedback(&mut self, v: impl IntoVal<f32>) -> &mut Self {
        match v.into_val() {
            Val::Fixed(f) => self.config.feedback = f,
            Val::Fn(mut f) => {
                self.config.feedback = f(Clock::ZERO);
                self.automations.push((delay_params::PARAM_FEEDBACK, f));
            }
        }
        self
    }

    pub fn mix(&mut self, v: impl IntoVal<f32>) -> &mut Self {
        match v.into_val() {
            Val::Fixed(f) => self.config.mix = f,
            Val::Fn(mut f) => {
                self.config.mix = f(Clock::ZERO);
                self.automations.push((delay_params::PARAM_MIX, f));
            }
        }
        self
    }

    pub fn ping_pong(&mut self) -> &mut Self {
        self.config.mode = DelayMode::PingPong;
        self
    }

    pub fn mode(&mut self, m: DelayMode) -> &mut Self {
        self.config.mode = m;
        self
    }

    pub fn enabled(&mut self, v: impl IntoVal<bool>) -> &mut Self {
        match v.into_val() {
            Val::Fixed(b) => self.initial_enabled = b,
            Val::Fn(mut f) => {
                self.initial_enabled = f(Clock::ZERO);
                self.enabled_auto = Some(f);
            }
        }
        self
    }

    pub fn into_config(self) -> DelayConfig {
        self.config
    }

    pub fn take_automations(&mut self) -> Vec<(u8, Box<dyn FnMut(Clock) -> f32 + Send>)> {
        std::mem::take(&mut self.automations)
    }

    pub fn take_enabled_auto(&mut self) -> Option<Box<dyn FnMut(Clock) -> bool + Send>> {
        self.enabled_auto.take()
    }

    pub fn initial_enabled(&self) -> bool {
        self.initial_enabled
    }
}

// ── Distortion ──

#[derive(Clone, Debug, PartialEq)]
pub struct DistortionConfig {
    pub drive: f32,
    pub mix: f32,
    pub mode: DistortionMode,
    pub bias: f32,
    pub tone: f32,
    pub output_gain: f32,
}

impl DistortionConfig {
    fn build(&self) -> Distortion {
        Distortion::new(self.drive, self.mix)
            .mode(self.mode)
            .bias(self.bias)
            .tone(self.tone)
            .output_gain(self.output_gain)
    }
}

pub struct DistortionBuilder {
    config: DistortionConfig,
    automations: Vec<(u8, Box<dyn FnMut(Clock) -> f32 + Send>)>,
    enabled_auto: Option<Box<dyn FnMut(Clock) -> bool + Send>>,
    initial_enabled: bool,
}

impl Default for DistortionBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl DistortionBuilder {
    pub fn new() -> Self {
        Self {
            config: DistortionConfig {
                drive: 1.0,
                mix: 1.0,
                mode: DistortionMode::SoftClip,
                bias: 0.0,
                tone: 1.0,
                output_gain: 1.0,
            },
            automations: Vec::new(),
            enabled_auto: None,
            initial_enabled: true,
        }
    }

    pub fn drive(&mut self, v: impl IntoVal<f32>) -> &mut Self {
        match v.into_val() {
            Val::Fixed(f) => self.config.drive = f,
            Val::Fn(mut f) => {
                self.config.drive = f(Clock::ZERO);
                self.automations.push((distortion_params::PARAM_DRIVE, f));
            }
        }
        self
    }

    pub fn mix(&mut self, v: impl IntoVal<f32>) -> &mut Self {
        match v.into_val() {
            Val::Fixed(f) => self.config.mix = f,
            Val::Fn(mut f) => {
                self.config.mix = f(Clock::ZERO);
                self.automations.push((distortion_params::PARAM_MIX, f));
            }
        }
        self
    }

    pub fn soft_clip(&mut self) -> &mut Self {
        self.config.mode = DistortionMode::SoftClip;
        self
    }

    pub fn hard_clip(&mut self) -> &mut Self {
        self.config.mode = DistortionMode::HardClip;
        self
    }

    pub fn tube(&mut self) -> &mut Self {
        self.config.mode = DistortionMode::Tube;
        self
    }

    pub fn fuzz(&mut self) -> &mut Self {
        self.config.mode = DistortionMode::Fuzz;
        self
    }

    pub fn saturate(&mut self) -> &mut Self {
        self.config.mode = DistortionMode::Saturate;
        self
    }

    pub fn bias(&mut self, v: impl IntoVal<f32>) -> &mut Self {
        match v.into_val() {
            Val::Fixed(f) => self.config.bias = f,
            Val::Fn(mut f) => {
                self.config.bias = f(Clock::ZERO);
                self.automations.push((distortion_params::PARAM_BIAS, f));
            }
        }
        self
    }

    pub fn tone(&mut self, v: impl IntoVal<f32>) -> &mut Self {
        match v.into_val() {
            Val::Fixed(f) => self.config.tone = f,
            Val::Fn(mut f) => {
                self.config.tone = f(Clock::ZERO);
                self.automations.push((distortion_params::PARAM_TONE, f));
            }
        }
        self
    }

    pub fn output(&mut self, v: impl IntoVal<f32>) -> &mut Self {
        match v.into_val() {
            Val::Fixed(f) => self.config.output_gain = f,
            Val::Fn(mut f) => {
                self.config.output_gain = f(Clock::ZERO);
                self.automations.push((distortion_params::PARAM_OUTPUT, f));
            }
        }
        self
    }

    pub fn enabled(&mut self, v: impl IntoVal<bool>) -> &mut Self {
        match v.into_val() {
            Val::Fixed(b) => self.initial_enabled = b,
            Val::Fn(mut f) => {
                self.initial_enabled = f(Clock::ZERO);
                self.enabled_auto = Some(f);
            }
        }
        self
    }

    pub fn into_config(self) -> DistortionConfig {
        self.config
    }

    pub fn take_automations(&mut self) -> Vec<(u8, Box<dyn FnMut(Clock) -> f32 + Send>)> {
        std::mem::take(&mut self.automations)
    }

    pub fn take_enabled_auto(&mut self) -> Option<Box<dyn FnMut(Clock) -> bool + Send>> {
        self.enabled_auto.take()
    }

    pub fn initial_enabled(&self) -> bool {
        self.initial_enabled
    }
}

// ── Chorus ──

#[derive(Clone, Debug, PartialEq)]
pub struct ChorusConfig {
    pub rate: f32,
    pub depth: f32,
    pub mix: f32,
}

impl ChorusConfig {
    fn build(&self, sample_rate: f32) -> Chorus {
        Chorus::new(self.rate, self.depth, self.mix, sample_rate)
    }
}

pub struct ChorusBuilder {
    config: ChorusConfig,
    automations: Vec<(u8, Box<dyn FnMut(Clock) -> f32 + Send>)>,
    enabled_auto: Option<Box<dyn FnMut(Clock) -> bool + Send>>,
    initial_enabled: bool,
}

impl Default for ChorusBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ChorusBuilder {
    pub fn new() -> Self {
        Self {
            config: ChorusConfig {
                rate: 1.0,
                depth: 0.003,
                mix: 0.3,
            },
            automations: Vec::new(),
            enabled_auto: None,
            initial_enabled: true,
        }
    }

    pub fn rate(&mut self, v: impl IntoVal<f32>) -> &mut Self {
        match v.into_val() {
            Val::Fixed(f) => self.config.rate = f,
            Val::Fn(mut f) => {
                self.config.rate = f(Clock::ZERO);
                self.automations.push((chorus_params::PARAM_RATE, f));
            }
        }
        self
    }

    pub fn depth(&mut self, v: impl IntoVal<f32>) -> &mut Self {
        match v.into_val() {
            Val::Fixed(f) => self.config.depth = f,
            Val::Fn(mut f) => {
                self.config.depth = f(Clock::ZERO);
                self.automations.push((chorus_params::PARAM_DEPTH, f));
            }
        }
        self
    }

    pub fn mix(&mut self, v: impl IntoVal<f32>) -> &mut Self {
        match v.into_val() {
            Val::Fixed(f) => self.config.mix = f,
            Val::Fn(mut f) => {
                self.config.mix = f(Clock::ZERO);
                self.automations.push((chorus_params::PARAM_MIX, f));
            }
        }
        self
    }

    pub fn enabled(&mut self, v: impl IntoVal<bool>) -> &mut Self {
        match v.into_val() {
            Val::Fixed(b) => self.initial_enabled = b,
            Val::Fn(mut f) => {
                self.initial_enabled = f(Clock::ZERO);
                self.enabled_auto = Some(f);
            }
        }
        self
    }

    pub fn into_config(self) -> ChorusConfig {
        self.config
    }

    pub fn take_automations(&mut self) -> Vec<(u8, Box<dyn FnMut(Clock) -> f32 + Send>)> {
        std::mem::take(&mut self.automations)
    }

    pub fn take_enabled_auto(&mut self) -> Option<Box<dyn FnMut(Clock) -> bool + Send>> {
        self.enabled_auto.take()
    }

    pub fn initial_enabled(&self) -> bool {
        self.initial_enabled
    }
}
