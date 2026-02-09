use crate::event::EventKind;
use crate::pattern::Pattern;
use crate::score::Time;

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

// ── Val: a parameter that is either fixed or clock-driven ──

pub enum Val {
    Fixed(f32),
    Fn(Box<dyn FnMut(Clock) -> f32 + Send>),
}

pub trait IntoVal {
    fn into_val(self) -> Val;
}

impl IntoVal for f32 {
    fn into_val(self) -> Val {
        Val::Fixed(self)
    }
}

impl<F: FnMut(Clock) -> f32 + Send + 'static> IntoVal for F {
    fn into_val(self) -> Val {
        Val::Fn(Box::new(self))
    }
}

// ── Internal types (audio thread) ──

pub(crate) type AutomationFn = Box<dyn FnMut(Clock) -> f32 + Send>;
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

    pub(crate) fn into_events(self) -> Vec<(Time, EventKind)> {
        self.events
    }
}
