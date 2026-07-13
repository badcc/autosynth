//! Fluent effect builders. Each `t.delay()` pushes a default `FxSpec` onto the
//! track immediately and returns a builder that borrows the track plus its fx
//! index; setters mutate that spec in place and record automations directly.
//! Same eager, chain-by-value style as `OscBuilder` — no closure form, no
//! `Drop` magic.
//!
//! Track builders take `impl IntoVal<f32>`, so a knob can be a number or a
//! `Clock` closure / `Shape`. Group builders take plain `f32`: group fx
//! automation is unsupported, so it is rejected by construction rather than
//! silently dropped.

use crate::dsp::effects::{DelayMode, DistortionMode};
use crate::live::scene::{GroupBuilder, SceneTrack};
use crate::model::fx::{
    ChorusCfg, DelayCfg, DistortionCfg, FxKind, FxSpec, ReverbCfg, chorus_slots, delay_slots,
    distortion_slots, reverb_slots,
};
use crate::model::param::IntoVal;

/// Generate a track builder (IntoVal setters + automation) and a group builder
/// (plain-`f32` setters) for one effect. Effect-specific setters (modes, timing)
/// are written by hand in additional `impl` blocks below.
macro_rules! fx_builder {
    (
        variant = $variant:ident, cfg = $cfg:ty, slots = $slots:ident,
        track = $tb:ident, group = $gb:ident,
        auto = [ $($method:ident => $field:ident @ $slot:ident),* $(,)? ]
    ) => {
        // ── Track builder: borrows the track, records automations ──
        pub struct $tb<'a> {
            pub(crate) track: &'a mut SceneTrack,
            pub(crate) idx: usize,
        }

        impl $tb<'_> {
            $(
                pub fn $method(self, v: impl IntoVal<f32>) -> Self {
                    self.track.fx_param(self.idx, $slots::$slot, v, |k, f| {
                        if let FxKind::$variant(c) = k {
                            c.$field = f;
                        }
                    });
                    self
                }
            )*

            /// Enable or disable this effect (buffers are preserved when only the
            /// flag changes).
            pub fn enabled(self, on: bool) -> Self {
                self.track.set_fx_enabled_flag(self.idx, on);
                self
            }
        }

        // ── Group builder: plain f32, no automation ──
        pub struct $gb<'a> {
            pub(crate) fx: &'a mut Vec<FxSpec>,
            pub(crate) idx: usize,
        }

        impl $gb<'_> {
            fn cfg(&mut self) -> &mut $cfg {
                match &mut self.fx[self.idx].kind {
                    FxKind::$variant(c) => c,
                    _ => unreachable!("group fx index mismatch"),
                }
            }

            $(
                pub fn $method(mut self, v: f32) -> Self {
                    self.cfg().$field = v;
                    self
                }
            )*

            pub fn enabled(self, on: bool) -> Self {
                self.fx[self.idx].enabled = on;
                self
            }
        }
    };
}

fx_builder! {
    variant = Delay, cfg = DelayCfg, slots = delay_slots,
    track = DelayBuilder, group = GroupDelayBuilder,
    auto = [ feedback => feedback @ PARAM_FEEDBACK, mix => mix @ PARAM_MIX ]
}

fx_builder! {
    variant = Chorus, cfg = ChorusCfg, slots = chorus_slots,
    track = ChorusBuilder, group = GroupChorusBuilder,
    auto = [ rate => rate @ PARAM_RATE, depth => depth @ PARAM_DEPTH, mix => mix @ PARAM_MIX ]
}

fx_builder! {
    variant = Distortion, cfg = DistortionCfg, slots = distortion_slots,
    track = DistortionBuilder, group = GroupDistortionBuilder,
    auto = [
        drive => drive @ PARAM_DRIVE,
        mix => mix @ PARAM_MIX,
        bias => bias @ PARAM_BIAS,
        tone => tone @ PARAM_TONE,
        output => output_gain @ PARAM_OUTPUT,
    ]
}

fx_builder! {
    variant = Reverb, cfg = ReverbCfg, slots = reverb_slots,
    track = ReverbBuilder, group = GroupReverbBuilder,
    auto = [
        size => size @ PARAM_SIZE,
        damp => damp @ PARAM_DAMP,
        mix => mix @ PARAM_MIX,
        width => width @ PARAM_WIDTH,
    ]
}

// ── Effect-specific (non-automatable) setters ──

impl DelayBuilder<'_> {
    pub fn beats(self, b: f32) -> Self {
        if let FxKind::Delay(c) = self.track.fx_kind_mut(self.idx) {
            c.beats = Some(b);
            c.seconds = None;
        }
        self
    }
    pub fn seconds(self, s: f32) -> Self {
        if let FxKind::Delay(c) = self.track.fx_kind_mut(self.idx) {
            c.seconds = Some(s);
            c.beats = None;
        }
        self
    }
    pub fn ping_pong(self) -> Self {
        if let FxKind::Delay(c) = self.track.fx_kind_mut(self.idx) {
            c.mode = DelayMode::PingPong;
        }
        self
    }
}

impl GroupDelayBuilder<'_> {
    pub fn beats(mut self, b: f32) -> Self {
        let c = self.cfg();
        c.beats = Some(b);
        c.seconds = None;
        self
    }
    pub fn seconds(mut self, s: f32) -> Self {
        let c = self.cfg();
        c.seconds = Some(s);
        c.beats = None;
        self
    }
    pub fn ping_pong(mut self) -> Self {
        self.cfg().mode = DelayMode::PingPong;
        self
    }
}

/// Distortion waveshaping modes, generated for both builder flavours.
macro_rules! distortion_modes {
    ($ty:ident) => {
        impl $ty<'_> {
            pub fn soft_clip(self) -> Self {
                self.set_mode(DistortionMode::SoftClip)
            }
            pub fn hard_clip(self) -> Self {
                self.set_mode(DistortionMode::HardClip)
            }
            pub fn tube(self) -> Self {
                self.set_mode(DistortionMode::Tube)
            }
            pub fn fuzz(self) -> Self {
                self.set_mode(DistortionMode::Fuzz)
            }
            pub fn saturate(self) -> Self {
                self.set_mode(DistortionMode::Saturate)
            }
        }
    };
}

impl DistortionBuilder<'_> {
    fn set_mode(self, mode: DistortionMode) -> Self {
        if let FxKind::Distortion(c) = self.track.fx_kind_mut(self.idx) {
            c.mode = mode;
        }
        self
    }
}

impl GroupDistortionBuilder<'_> {
    fn set_mode(mut self, mode: DistortionMode) -> Self {
        self.cfg().mode = mode;
        self
    }
}

distortion_modes!(DistortionBuilder);
distortion_modes!(GroupDistortionBuilder);

// ── Constructors: pushed onto the track / group and handed a builder ──

impl SceneTrack {
    pub fn delay(&mut self) -> DelayBuilder<'_> {
        let idx = self.push_fx_default(FxKind::Delay(DelayCfg::default()));
        DelayBuilder { track: self, idx }
    }
    pub fn chorus(&mut self) -> ChorusBuilder<'_> {
        let idx = self.push_fx_default(FxKind::Chorus(ChorusCfg::default()));
        ChorusBuilder { track: self, idx }
    }
    pub fn distortion(&mut self) -> DistortionBuilder<'_> {
        let idx = self.push_fx_default(FxKind::Distortion(DistortionCfg::default()));
        DistortionBuilder { track: self, idx }
    }
    pub fn reverb(&mut self) -> ReverbBuilder<'_> {
        let idx = self.push_fx_default(FxKind::Reverb(ReverbCfg::default()));
        ReverbBuilder { track: self, idx }
    }
}

impl GroupBuilder {
    pub fn delay(&mut self) -> GroupDelayBuilder<'_> {
        let idx = self.push_fx_default(FxKind::Delay(DelayCfg::default()));
        GroupDelayBuilder { fx: self.fx_mut(), idx }
    }
    pub fn chorus(&mut self) -> GroupChorusBuilder<'_> {
        let idx = self.push_fx_default(FxKind::Chorus(ChorusCfg::default()));
        GroupChorusBuilder { fx: self.fx_mut(), idx }
    }
    pub fn distortion(&mut self) -> GroupDistortionBuilder<'_> {
        let idx = self.push_fx_default(FxKind::Distortion(DistortionCfg::default()));
        GroupDistortionBuilder { fx: self.fx_mut(), idx }
    }
    pub fn reverb(&mut self) -> GroupReverbBuilder<'_> {
        let idx = self.push_fx_default(FxKind::Reverb(ReverbCfg::default()));
        GroupReverbBuilder { fx: self.fx_mut(), idx }
    }
}
