use crate::envelope::RetriggerMode;
use crate::event::{EventKind, SynthParam};
use crate::filter::FilterType;
use crate::pattern::Pattern;
use crate::score::Time;
use crate::waveform::Waveform;

// ── Clock: timing context passed to parameter closures ──

/// Timing context for parameter automation closures.
#[derive(Clone, Copy, Debug)]
pub struct Clock {
    /// Global beat position since playback start.
    pub beat: f32,
    /// Beat position within the current loop (0..loop_length).
    /// Equal to `beat` if the track is not looping.
    pub local: f32,
    /// Current loop iteration (0 on first play, increments each loop).
    pub iteration: u32,
}

impl Clock {
    pub(crate) const ZERO: Clock = Clock {
        beat: 0.0,
        local: 0.0,
        iteration: 0,
    };
}

// ── Val<T>: a parameter that is either fixed or clock-driven ──

pub enum Val<T> {
    Fixed(T),
    Fn(Box<dyn FnMut(Clock) -> T + Send>),
}

pub trait IntoVal<T> {
    fn into_val(self) -> Val<T>;
}

macro_rules! impl_into_val {
    ($t:ty) => {
        impl IntoVal<$t> for $t {
            fn into_val(self) -> Val<$t> {
                Val::Fixed(self)
            }
        }

        impl<F: FnMut(Clock) -> $t + Send + 'static> IntoVal<$t> for F {
            fn into_val(self) -> Val<$t> {
                Val::Fn(Box::new(self))
            }
        }
    };
}

impl_into_val!(f32);
impl_into_val!(bool);
impl_into_val!(Waveform);
impl_into_val!(FilterType);
impl_into_val!(RetriggerMode);

// ── OscParam: automatable oscillator parameters ──

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OscParam {
    Detune,
    Level,
}

// ── AutoCmd: what an automation closure produces ──

#[derive(Clone, Copy, Debug)]
pub(crate) enum AutoCmd {
    Synth(SynthParam, f32),
    FilterType(FilterType),
    Retrigger(RetriggerMode),
    OscParam { osc_index: usize, param: OscParam, value: f32 },
    OscWaveform { osc_index: usize, waveform: Waveform },
    FxParam { fx_index: usize, slot: u8, value: f32 },
    FxEnabled { fx_index: usize, enabled: bool },
}

// ── Automation: a closure that produces an AutoCmd ──

pub(crate) type Automation = Box<dyn FnMut(Clock) -> AutoCmd + Send>;

// ── Internal types (audio thread) ──

pub(crate) type PatternFn = Box<dyn FnMut(&mut Phrase) + Send>;

// ── Phrase: note builder used inside every() closures ──

pub struct Phrase {
    events: Vec<(Time, EventKind)>,
    /// Global beat at the start of this loop iteration.
    pub beat: f32,
    /// Loop iteration count (0 on first play).
    pub iteration: u32,
}

impl Phrase {
    pub fn new() -> Self {
        Self {
            events: Vec::new(),
            beat: 0.0,
            iteration: 0,
        }
    }

    pub(crate) fn with_context(beat: f32, iteration: u32) -> Self {
        Self {
            events: Vec::new(),
            beat,
            iteration,
        }
    }

    pub fn note(&mut self, time: Time, note: u8, vel: f32, dur: f32) -> &mut Self {
        let end = match time {
            Time::Seconds(s) => Time::Seconds(s + dur),
            Time::Beats(b) => Time::Beats(b + dur),
        };
        self.events.push((time, EventKind::NoteOn { note, vel }));
        self.events.push((end, EventKind::NoteOff { note }));
        self
    }

    pub fn chord(&mut self, time: Time, notes: &[u8], vel: f32, dur: f32) -> &mut Self {
        for &n in notes {
            self.note(time, n, vel, dur);
        }
        self
    }

    pub fn pattern(&mut self, start: Time, pattern: &Pattern) -> &mut Self {
        for pn in pattern.notes() {
            let time = match start {
                Time::Seconds(s) => Time::Seconds(s + pn.beat),
                Time::Beats(b) => Time::Beats(b + pn.beat),
            };
            self.note(time, pn.note, pn.velocity, pn.duration);
        }
        self
    }

    /// Step-sequencer style pattern. Each character = one 16th note (0.25 beats).
    /// `x` = hit at `vel`, `X` = accent at `(vel * 1.3).min(1.0)`, anything else = rest.
    pub fn steps(&mut self, note: u8, pattern: &str, vel: f32) -> &mut Self {
        let step = 0.25_f32; // 16th note
        let dur = 0.225_f32; // 90% of step, slight gap
        for (i, ch) in pattern.chars().enumerate() {
            let v = match ch {
                'x' => vel,
                'X' => (vel * 1.3).min(1.0),
                _ => continue,
            };
            let beat = i as f32 * step;
            self.note(Time::Beats(beat), note, v, dur);
        }
        self
    }

    pub(crate) fn into_events(self) -> Vec<(Time, EventKind)> {
        self.events
    }
}
