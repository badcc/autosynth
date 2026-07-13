//! Fluent builders for the four effects. Each produces a diffable `FxKind`
//! config plus any parameter automations, using the same `IntoVal` sugar as the
//! synth params: `d.feedback(0.4)` or `d.feedback(|c| c.sin(4.0))`.

use crate::dsp::effects::{DelayMode, DistortionMode};
use crate::model::fx::{
    ChorusCfg, DelayCfg, DistortionCfg, FxKind, ReverbCfg, chorus_slots, delay_slots,
    distortion_slots, reverb_slots,
};
use crate::model::param::{AutomationFn, IntoVal, Val};

/// The result of an fx builder: its config, enable flag, and slot automations.
pub struct FxResult {
    pub kind: FxKind,
    pub enabled: bool,
    pub automations: Vec<(u8, AutomationFn)>,
}

/// Set a config field from a `Val`, recording the automation if it's a closure.
macro_rules! set_param {
    ($self:ident, $field:expr, $slot:expr, $v:expr) => {
        match $v.into_val() {
            Val::Fixed(f) => $field = f,
            Val::Fn(mut f) => {
                $field = f(crate::music::Clock::ZERO);
                $self.automations.push(($slot, f));
            }
        }
    };
}

// ── Delay ──

pub struct DelayBuilder {
    cfg: DelayCfg,
    enabled: bool,
    automations: Vec<(u8, AutomationFn)>,
}

impl DelayBuilder {
    pub(crate) fn new() -> Self {
        Self {
            cfg: DelayCfg::default(),
            enabled: true,
            automations: Vec::new(),
        }
    }

    pub fn beats(&mut self, b: f32) -> &mut Self {
        self.cfg.beats = Some(b);
        self.cfg.seconds = None;
        self
    }

    pub fn seconds(&mut self, s: f32) -> &mut Self {
        self.cfg.seconds = Some(s);
        self.cfg.beats = None;
        self
    }

    pub fn feedback(&mut self, v: impl IntoVal<f32>) -> &mut Self {
        set_param!(self, self.cfg.feedback, delay_slots::PARAM_FEEDBACK, v);
        self
    }

    pub fn mix(&mut self, v: impl IntoVal<f32>) -> &mut Self {
        set_param!(self, self.cfg.mix, delay_slots::PARAM_MIX, v);
        self
    }

    pub fn ping_pong(&mut self) -> &mut Self {
        self.cfg.mode = DelayMode::PingPong;
        self
    }

    pub fn enabled(&mut self, on: bool) -> &mut Self {
        self.enabled = on;
        self
    }

    pub(crate) fn finish(self) -> FxResult {
        FxResult {
            kind: FxKind::Delay(self.cfg),
            enabled: self.enabled,
            automations: self.automations,
        }
    }
}

// ── Chorus ──

pub struct ChorusBuilder {
    cfg: ChorusCfg,
    enabled: bool,
    automations: Vec<(u8, AutomationFn)>,
}

impl ChorusBuilder {
    pub(crate) fn new() -> Self {
        Self {
            cfg: ChorusCfg::default(),
            enabled: true,
            automations: Vec::new(),
        }
    }

    pub fn rate(&mut self, v: impl IntoVal<f32>) -> &mut Self {
        set_param!(self, self.cfg.rate, chorus_slots::PARAM_RATE, v);
        self
    }

    pub fn depth(&mut self, v: impl IntoVal<f32>) -> &mut Self {
        set_param!(self, self.cfg.depth, chorus_slots::PARAM_DEPTH, v);
        self
    }

    pub fn mix(&mut self, v: impl IntoVal<f32>) -> &mut Self {
        set_param!(self, self.cfg.mix, chorus_slots::PARAM_MIX, v);
        self
    }

    pub fn enabled(&mut self, on: bool) -> &mut Self {
        self.enabled = on;
        self
    }

    pub(crate) fn finish(self) -> FxResult {
        FxResult {
            kind: FxKind::Chorus(self.cfg),
            enabled: self.enabled,
            automations: self.automations,
        }
    }
}

// ── Distortion ──

pub struct DistortionBuilder {
    cfg: DistortionCfg,
    enabled: bool,
    automations: Vec<(u8, AutomationFn)>,
}

impl DistortionBuilder {
    pub(crate) fn new() -> Self {
        Self {
            cfg: DistortionCfg::default(),
            enabled: true,
            automations: Vec::new(),
        }
    }

    pub fn drive(&mut self, v: impl IntoVal<f32>) -> &mut Self {
        set_param!(self, self.cfg.drive, distortion_slots::PARAM_DRIVE, v);
        self
    }

    pub fn mix(&mut self, v: impl IntoVal<f32>) -> &mut Self {
        set_param!(self, self.cfg.mix, distortion_slots::PARAM_MIX, v);
        self
    }

    pub fn bias(&mut self, v: impl IntoVal<f32>) -> &mut Self {
        set_param!(self, self.cfg.bias, distortion_slots::PARAM_BIAS, v);
        self
    }

    pub fn tone(&mut self, v: impl IntoVal<f32>) -> &mut Self {
        set_param!(self, self.cfg.tone, distortion_slots::PARAM_TONE, v);
        self
    }

    pub fn output(&mut self, v: impl IntoVal<f32>) -> &mut Self {
        set_param!(self, self.cfg.output_gain, distortion_slots::PARAM_OUTPUT, v);
        self
    }

    pub fn soft_clip(&mut self) -> &mut Self {
        self.cfg.mode = DistortionMode::SoftClip;
        self
    }

    pub fn hard_clip(&mut self) -> &mut Self {
        self.cfg.mode = DistortionMode::HardClip;
        self
    }

    pub fn tube(&mut self) -> &mut Self {
        self.cfg.mode = DistortionMode::Tube;
        self
    }

    pub fn fuzz(&mut self) -> &mut Self {
        self.cfg.mode = DistortionMode::Fuzz;
        self
    }

    pub fn saturate(&mut self) -> &mut Self {
        self.cfg.mode = DistortionMode::Saturate;
        self
    }

    pub fn enabled(&mut self, on: bool) -> &mut Self {
        self.enabled = on;
        self
    }

    pub(crate) fn finish(self) -> FxResult {
        FxResult {
            kind: FxKind::Distortion(self.cfg),
            enabled: self.enabled,
            automations: self.automations,
        }
    }
}

// ── Reverb ──

pub struct ReverbBuilder {
    cfg: ReverbCfg,
    enabled: bool,
    automations: Vec<(u8, AutomationFn)>,
}

impl ReverbBuilder {
    pub(crate) fn new() -> Self {
        Self {
            cfg: ReverbCfg::default(),
            enabled: true,
            automations: Vec::new(),
        }
    }

    pub fn size(&mut self, v: impl IntoVal<f32>) -> &mut Self {
        set_param!(self, self.cfg.size, reverb_slots::PARAM_SIZE, v);
        self
    }

    pub fn damp(&mut self, v: impl IntoVal<f32>) -> &mut Self {
        set_param!(self, self.cfg.damp, reverb_slots::PARAM_DAMP, v);
        self
    }

    pub fn mix(&mut self, v: impl IntoVal<f32>) -> &mut Self {
        set_param!(self, self.cfg.mix, reverb_slots::PARAM_MIX, v);
        self
    }

    pub fn width(&mut self, v: impl IntoVal<f32>) -> &mut Self {
        set_param!(self, self.cfg.width, reverb_slots::PARAM_WIDTH, v);
        self
    }

    pub fn enabled(&mut self, on: bool) -> &mut Self {
        self.enabled = on;
        self
    }

    pub(crate) fn finish(self) -> FxResult {
        FxResult {
            kind: FxKind::Reverb(self.cfg),
            enabled: self.enabled,
            automations: self.automations,
        }
    }
}
