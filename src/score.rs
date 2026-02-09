use crate::event::{Event, EventKind, Param};
use crate::patch::Patch;
use crate::pattern::Pattern;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tempo {
    pub bpm: f32,
}

impl Tempo {
    pub fn new(bpm: f32) -> Self {
        Self { bpm }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Time {
    Seconds(f32),
    Beats(f32),
}

/// Create a beat-based time
pub fn b(beats: f32) -> Time {
    Time::Beats(beats)
}

/// Create a bar+beat time (assumes 4/4)
pub fn bar(n: u32, beat: f32) -> Time {
    Time::Beats(n as f32 * 4.0 + beat)
}

#[derive(Clone, Debug, Default)]
pub struct Score {
    events: Vec<(Time, EventKind)>,
}

impl Score {
    pub fn new() -> Self {
        Self { events: Vec::new() }
    }

    /// Create a Score from a pre-built event list (used by the scene diff system).
    pub fn from_events(events: Vec<(Time, EventKind)>) -> Self {
        Self { events }
    }

    pub fn note(&mut self, time: Time, note: u8, vel: f32, dur: f32) -> &mut Self {
        let end_time = match time {
            Time::Seconds(s) => Time::Seconds(s + dur),
            Time::Beats(b) => Time::Beats(b + dur),
        };
        self.events.push((time, EventKind::NoteOn { note, vel }));
        self.events.push((end_time, EventKind::NoteOff { note }));
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

    pub fn param(&mut self, time: Time, param: Param, value: f32) -> &mut Self {
        self.events.push((time, EventKind::Param { param, value }));
        self
    }

    pub fn patch(&mut self, time: Time, patch: Patch) -> &mut Self {
        self.events.push((time, EventKind::SetPatch { patch }));
        self
    }

    pub fn events(&self) -> &[(Time, EventKind)] {
        &self.events
    }

    pub(crate) fn to_sequence(&self, tempo: Tempo, sample_rate: f32) -> Sequence {
        let mut out: Vec<Event> = self
            .events
            .iter()
            .map(|(t, k)| Event {
                sample: time_to_sample(*t, tempo, sample_rate),
                kind: k.clone(),
            })
            .collect();
        out.sort_by_key(|e| e.sample);
        Sequence { events: out }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Sequence {
    events: Vec<Event>,
}

impl Sequence {
    pub(crate) fn player(self) -> SequencePlayer {
        SequencePlayer::new(self)
    }

    pub(crate) fn loop_player(self, loop_len_samples: u64) -> SequencePlayer {
        SequencePlayer::with_loop(self, loop_len_samples)
    }
}

pub(crate) struct SequencePlayer {
    seq: Sequence,
    index: usize,
    loop_len: Option<u64>,
    base: u64,
}

impl SequencePlayer {
    pub fn new(seq: Sequence) -> Self {
        Self {
            seq,
            index: 0,
            loop_len: None,
            base: 0,
        }
    }

    pub fn with_loop(seq: Sequence, loop_len_samples: u64) -> Self {
        Self {
            seq,
            index: 0,
            loop_len: Some(loop_len_samples.max(1)),
            base: 0,
        }
    }

    pub fn advance_loop(&mut self, sample: u64) {
        if let Some(len) = self.loop_len {
            while sample >= self.base + len {
                self.base = self.base.saturating_add(len);
                self.index = 0;
            }
        }
    }

    pub fn peek(&self) -> Option<Event> {
        self.seq.events.get(self.index).map(|e| Event {
            sample: e.sample + self.base,
            kind: e.kind.clone(),
        })
    }

    pub fn pop(&mut self) -> Option<Event> {
        if self.index >= self.seq.events.len() {
            None
        } else {
            let e = &self.seq.events[self.index];
            self.index += 1;
            Some(Event {
                sample: e.sample + self.base,
                kind: e.kind.clone(),
            })
        }
    }
}

fn time_to_sample(time: Time, tempo: Tempo, sample_rate: f32) -> u64 {
    match time {
        Time::Seconds(s) => (s * sample_rate).max(0.0) as u64,
        Time::Beats(b) => {
            let seconds = b * 60.0 / tempo.bpm.max(1.0);
            (seconds * sample_rate).max(0.0) as u64
        }
    }
}
