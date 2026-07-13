use crate::music::{Clock, Phrase};

/// A stable, track-scoped handle to one automatable parameter.
///
/// Every continuous parameter — synth envelope/filter, per-oscillator level and
/// detune, per-effect params, effect enable, and the mixer gain/pan — is named
/// by a `ParamId`. One representation replaces the old pile of `AutoCmd`
/// variants and per-effect `u8` slot plumbing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ParamId {
    Attack,
    Decay,
    Sustain,
    Release,
    Cutoff,
    Resonance,
    LfoRate,
    LfoDepth,
    /// Mixer gain (linear).
    Gain,
    /// Mixer pan, `-1.0..1.0`.
    Pan,
    /// Level of oscillator `index`.
    OscLevel(usize),
    /// Detune (semitones) of oscillator `index`.
    OscDetune(usize),
    /// Parameter `slot` of effect `index` in the fx chain.
    Fx { index: usize, slot: u8 },
    /// Enable/disable effect `index` (value `> 0.5` = enabled).
    FxEnabled(usize),
}

/// A closure evaluated at control rate to drive one parameter.
pub type AutomationFn = Box<dyn FnMut(Clock) -> f32 + Send>;

/// A parameter bound to an automation closure.
pub type Automation = (ParamId, AutomationFn);

/// A looping-pattern closure. Re-run at each loop boundary so `rand` re-rolls
/// and `p.iteration` can evolve the phrase.
pub type PatternFn = Box<dyn FnMut(&mut Phrase) + Send>;

/// A builder-surface parameter value: either a fixed number or a clock-driven
/// automation closure. One method (`t.cutoff(..)`) accepts both — the type
/// system decides. This *is* the automation API.
pub enum Val<T> {
    Fixed(T),
    Fn(Box<dyn FnMut(Clock) -> T + Send>),
}

pub trait IntoVal<T> {
    fn into_val(self) -> Val<T>;
}

impl IntoVal<f32> for f32 {
    fn into_val(self) -> Val<f32> {
        Val::Fixed(self)
    }
}

impl<F: FnMut(Clock) -> f32 + Send + 'static> IntoVal<f32> for F {
    fn into_val(self) -> Val<f32> {
        Val::Fn(Box::new(self))
    }
}
