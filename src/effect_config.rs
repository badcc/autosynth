use crate::effects::{Chorus, Delay, DelayMode, Distortion, DistortionMode, Effect};

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

    pub fn feedback(&mut self, f: f32) -> &mut Self {
        self.config.feedback = f;
        self
    }

    pub fn mix(&mut self, m: f32) -> &mut Self {
        self.config.mix = m;
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

    pub fn into_config(self) -> DelayConfig {
        self.config
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
        }
    }

    pub fn drive(&mut self, v: f32) -> &mut Self {
        self.config.drive = v;
        self
    }

    pub fn mix(&mut self, v: f32) -> &mut Self {
        self.config.mix = v;
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

    pub fn bias(&mut self, v: f32) -> &mut Self {
        self.config.bias = v;
        self
    }

    pub fn tone(&mut self, v: f32) -> &mut Self {
        self.config.tone = v;
        self
    }

    pub fn output(&mut self, v: f32) -> &mut Self {
        self.config.output_gain = v;
        self
    }

    pub fn into_config(self) -> DistortionConfig {
        self.config
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
        }
    }

    pub fn rate(&mut self, v: f32) -> &mut Self {
        self.config.rate = v;
        self
    }

    pub fn depth(&mut self, v: f32) -> &mut Self {
        self.config.depth = v;
        self
    }

    pub fn mix(&mut self, v: f32) -> &mut Self {
        self.config.mix = v;
        self
    }

    pub fn into_config(self) -> ChorusConfig {
        self.config
    }
}
