//! The chain builder: `t.fx(|c| { c.drive(3.0); c.delay(N8.dotted()).mix(0.2); })`.
//!
//! Every effect method pushes a node with default parameters and returns a
//! guard whose setters take `impl Into<Signal>` — a number, an LFO, a curve, a
//! knob. Chains are plain functions (`fn space(c: &mut Chain)`), so a library
//! of them is just a module of functions, and `t.fx(chains::space)` reuses one.

use crate::dsp::effects::delay::DelayMode;
use crate::dsp::effects::distortion::DistortionMode;
use crate::dsp::effects::{chorus, compressor, crush, delay, distortion, eq, filter, gate, limiter, phaser, reverb, width};
use crate::dsp::filter::FilterType;
use crate::live::track_builder::Bus;
use crate::model::chain::{ChainSpec, CustomEffect, CustomFx, FxKind, FxNode};
use crate::model::track::BusId;
use crate::music::signal::Signal;

#[derive(Default)]
pub struct Chain {
    nodes: Vec<FxNode>,
}

impl Chain {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build a chain by running `f` on an empty one.
    pub fn from_fn(f: impl FnOnce(&mut Chain)) -> ChainSpec {
        let mut c = Chain::new();
        f(&mut c);
        c.into_spec()
    }

    pub fn into_spec(self) -> ChainSpec {
        ChainSpec(self.nodes)
    }

    fn push(&mut self, kind: FxKind) -> &mut FxNode {
        self.nodes.push(kind.node());
        self.nodes.last_mut().expect("just pushed")
    }

    fn push_with(&mut self, kind: FxKind, slot: usize, v: impl Into<Signal>) -> &mut FxNode {
        let node = self.push(kind);
        set(node, slot, v.into());
        node
    }

    /// Tempo-synced delay; `time` in beats (`N8.dotted()`).
    pub fn delay(&mut self, time: impl Into<Signal>) -> DelayFx<'_> {
        DelayFx(self.push_with(FxKind::Delay(DelayMode::Normal), delay::TIME, time))
    }

    pub fn reverb(&mut self) -> ReverbFx<'_> {
        ReverbFx(self.push(FxKind::Reverb))
    }

    pub fn chorus(&mut self) -> ChorusFx<'_> {
        ChorusFx(self.push(FxKind::Chorus))
    }

    /// Waveshaping distortion (soft clip by default; see the mode methods).
    pub fn drive(&mut self, amount: impl Into<Signal>) -> DriveFx<'_> {
        DriveFx(self.push_with(FxKind::Drive(DistortionMode::SoftClip), distortion::DRIVE, amount))
    }

    pub fn lowpass(&mut self, cutoff: impl Into<Signal>) -> FilterKnobs<'_> {
        FilterKnobs(self.push_with(FxKind::Filter(FilterType::Lowpass), filter::CUTOFF, cutoff))
    }

    pub fn highpass(&mut self, cutoff: impl Into<Signal>) -> FilterKnobs<'_> {
        FilterKnobs(self.push_with(FxKind::Filter(FilterType::Highpass), filter::CUTOFF, cutoff))
    }

    pub fn bandpass(&mut self, cutoff: impl Into<Signal>) -> FilterKnobs<'_> {
        FilterKnobs(self.push_with(FxKind::Filter(FilterType::Bandpass), filter::CUTOFF, cutoff))
    }

    /// The resonant ladder lowpass, as an effect.
    pub fn ladder(&mut self, cutoff: impl Into<Signal>) -> FilterKnobs<'_> {
        FilterKnobs(self.push_with(FxKind::Filter(FilterType::Ladder), filter::CUTOFF, cutoff))
    }

    /// Three-band EQ (gains in dB).
    pub fn eq(&mut self) -> EqFx<'_> {
        EqFx(self.push(FxKind::Eq))
    }

    pub fn comp(&mut self) -> CompFx<'_> {
        CompFx(self.push(FxKind::Comp))
    }

    /// Trance gate: `x` open, `.` closed, one character per `grid` beats
    /// (default a 16th), locked to song position.
    pub fn gate(&mut self, pattern: &str) -> GateFx<'_> {
        GateFx(self.push(FxKind::Gate(pattern.to_string())))
    }

    pub fn phaser(&mut self) -> PhaserFx<'_> {
        PhaserFx(self.push(FxKind::Phaser))
    }

    pub fn crush(&mut self) -> CrushFx<'_> {
        CrushFx(self.push(FxKind::Crush))
    }

    /// Stereo width: 0 mono, 1 unchanged, 2 extra wide.
    pub fn width(&mut self, w: impl Into<Signal>) -> WidthFx<'_> {
        WidthFx(self.push_with(FxKind::Width, width::WIDTH, w))
    }

    pub fn limit(&mut self) -> LimitFx<'_> {
        LimitFx(self.push(FxKind::Limit))
    }

    /// A user-defined effect.
    pub fn custom<T: CustomEffect + PartialEq>(&mut self, fx: T) -> CustomKnobs<'_> {
        CustomKnobs(self.push(FxKind::Custom(CustomFx::new(fx))))
    }

    /// Send `amount ×` the signal at this point to a bus; the chain continues.
    pub fn send<F: Fn(&mut Bus) + 'static>(&mut self, _bus: F, amount: impl Into<Signal>) {
        self.nodes.push(FxNode::Send { bus: BusId::of::<F>(), amount: amount.into() });
    }

    /// Two parallel branches, summed: `c.parallel(|_| {}, |w| { w.reverb(); })`
    /// is a dry/wet split.
    pub fn parallel(&mut self, a: impl FnOnce(&mut Chain), b: impl FnOnce(&mut Chain)) {
        self.nodes.push(FxNode::Parallel(vec![Chain::from_fn(a), Chain::from_fn(b)]));
    }
}

fn set(node: &mut FxNode, slot: usize, v: Signal) {
    if let FxNode::Fx { params, .. } = node
        && let Some(p) = params.get_mut(slot)
    {
        *p = v;
    }
}

fn set_enabled(node: &mut FxNode, on: bool) {
    if let FxNode::Fx { enabled, .. } = node {
        *enabled = on;
    }
}

/// Generate a guard with one signal setter per parameter slot.
macro_rules! guard {
    ($(#[$doc:meta])* $name:ident { $($method:ident => $slot:path),* $(,)? }) => {
        $(#[$doc])*
        pub struct $name<'a>(&'a mut FxNode);

        impl $name<'_> {
            $(
                pub fn $method(self, v: impl Into<Signal>) -> Self {
                    set(self.0, $slot, v.into());
                    self
                }
            )*

            /// Bypass without losing state (tails keep ringing when re-enabled).
            pub fn enabled(self, on: bool) -> Self {
                set_enabled(self.0, on);
                self
            }
        }
    };
}

guard!(DelayFx { time => delay::TIME, feedback => delay::FEEDBACK, mix => delay::MIX });
guard!(ReverbFx { size => reverb::SIZE, damp => reverb::DAMP, mix => reverb::MIX, width => reverb::WIDTH });
guard!(ChorusFx { rate => chorus::RATE, depth => chorus::DEPTH, mix => chorus::MIX });
guard!(DriveFx {
    drive => distortion::DRIVE,
    mix => distortion::MIX,
    bias => distortion::BIAS,
    tone => distortion::TONE,
    output => distortion::OUTPUT,
});
guard!(FilterKnobs { cutoff => filter::CUTOFF, res => filter::RES });
guard!(EqFx {
    low => eq::LOW,
    mid => eq::MID,
    high => eq::HIGH,
    low_freq => eq::LOW_FREQ,
    mid_freq => eq::MID_FREQ,
    high_freq => eq::HIGH_FREQ,
});
guard!(CompFx {
    threshold => compressor::THRESHOLD,
    ratio => compressor::RATIO,
    attack => compressor::ATTACK,
    release => compressor::RELEASE,
    makeup => compressor::MAKEUP,
    mix => compressor::MIX,
});
guard!(GateFx { depth => gate::DEPTH, grid => gate::GRID, smooth => gate::SMOOTH });
guard!(PhaserFx { rate => phaser::RATE, depth => phaser::DEPTH, feedback => phaser::FEEDBACK, mix => phaser::MIX });
guard!(CrushFx { bits => crush::BITS, down => crush::DOWN, mix => crush::MIX });
guard!(WidthFx { width => width::WIDTH });
guard!(LimitFx { ceiling => limiter::CEILING, release => limiter::RELEASE });

impl DelayFx<'_> {
    /// Echoes alternate left and right.
    pub fn ping_pong(self) -> Self {
        if let FxNode::Fx { kind: FxKind::Delay(mode), .. } = self.0 {
            *mode = DelayMode::PingPong;
        }
        self
    }
}

impl DriveFx<'_> {
    fn mode(self, m: DistortionMode) -> Self {
        if let FxNode::Fx { kind: FxKind::Drive(mode), .. } = self.0 {
            *mode = m;
        }
        self
    }

    pub fn soft_clip(self) -> Self {
        self.mode(DistortionMode::SoftClip)
    }

    pub fn hard_clip(self) -> Self {
        self.mode(DistortionMode::HardClip)
    }

    pub fn tube(self) -> Self {
        self.mode(DistortionMode::Tube)
    }

    pub fn fuzz(self) -> Self {
        self.mode(DistortionMode::Fuzz)
    }

    pub fn saturate(self) -> Self {
        self.mode(DistortionMode::Saturate)
    }
}

/// Guard for a custom effect: parameters by slot index.
pub struct CustomKnobs<'a>(&'a mut FxNode);

impl CustomKnobs<'_> {
    pub fn param(self, slot: usize, v: impl Into<Signal>) -> Self {
        set(self.0, slot, v.into());
        self
    }

    pub fn enabled(self, on: bool) -> Self {
        set_enabled(self.0, on);
        self
    }
}
