use crate::event::EventKind;
use crate::pattern::Pattern;
use crate::score::Time;

// ── Val: a parameter that is either fixed or beat-driven ──

pub enum Val {
    Fixed(f32),
    Fn(Box<dyn FnMut(f32) -> f32 + Send>),
}

pub trait IntoVal {
    fn into_val(self) -> Val;
}

impl IntoVal for f32 {
    fn into_val(self) -> Val {
        Val::Fixed(self)
    }
}

impl<F: FnMut(f32) -> f32 + Send + 'static> IntoVal for F {
    fn into_val(self) -> Val {
        Val::Fn(Box::new(self))
    }
}

// ── Internal types (audio thread) ──

pub(crate) type AutomationFn = Box<dyn FnMut(f32) -> f32 + Send>;
pub(crate) type PatternFn = Box<dyn FnMut(&mut Phrase) + Send>;

// ── Phrase: note builder used inside every() closures ──

pub struct Phrase {
    events: Vec<(Time, EventKind)>,
}

impl Phrase {
    pub fn new() -> Self {
        Self { events: Vec::new() }
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
