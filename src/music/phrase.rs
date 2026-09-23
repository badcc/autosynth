//! `Phrase`: the note container a track's `play` closure fills, once per loop.
//!
//! Notes land at a *cursor* that advances as you place them, on a *grid* (a
//! 16th by default) that mini-notation strings step through:
//!
//! ```ignore
//! t.play(bars(1), |p| {
//!     p.hits(kick, "x... x... x... x...");
//!     p.seq("1 . b3 5  8 . 5 b3").slide("..x. ....").vel("X... ..X.");
//!     p.every(4, |p| p.transpose(12));
//! });
//! ```
//!
//! Every placement returns a [`Placed`] guard whose *lanes* (`.vel`, `.slide`,
//! `.cutoff`, …) take a number, a signal sampled at each note, or a lane string
//! cycled over the placed notes. The closure re-runs each loop: `p.cycle` counts
//! loops on the song grid, `p.during(DROP)` reads the form, and the seeded RNG
//! re-rolls per cycle yet renders reproducibly.

use std::ops::Range;

use crate::music::duration::N16;
use crate::music::form::Section;
use crate::music::harmony::{MAJOR, spread, voice_lead};
use crate::music::mini::{self, Mode};
use crate::music::notes::C4;
use crate::music::pitch::Key;
use crate::music::signal::{EvalCtx, Program, Signal, VoiceInputs};

/// Per-note parameter locks: overrides for this note's voice.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Locks {
    pub cutoff: Option<f32>,
    pub res: Option<f32>,
    pub decay: Option<f32>,
    pub gain: Option<f32>,
}

/// One scheduled note. `beat` is loop-local; the scheduler turns each event
/// into a note-on plus a note-off obligation `dur` beats later.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Event {
    pub beat: f32,
    pub dur: f32,
    pub note: u8,
    pub vel: f32,
    /// Slide into the next note: the gate is held over it and a mono voice
    /// glides instead of retriggering.
    pub slide: bool,
    pub locks: Locks,
}

impl Event {
    pub fn new(beat: f32, note: u8, vel: f32, dur: f32) -> Self {
        Self { beat, dur, note, vel, slide: false, locks: Locks::default() }
    }
}

const DEFAULT_VEL: f32 = 0.8;
const ACCENT_VEL: f32 = 1.0;
const GHOST_VEL: f32 = 0.35;
/// Fraction of a step a note sounds for, leaving a small gap before the next.
const GATE: f32 = 0.9;
/// How far a sliding note's gate overlaps the note it slides into.
const SLIDE_OVERLAP: f32 = 0.02;

/// What a phrase is generated in: where in the song this loop starts, which
/// loop it is, the harmonic context, and the RNG seed.
#[derive(Clone, Copy)]
pub struct PhraseCtx {
    /// Song beat at the start of this loop.
    pub beat: f64,
    /// Loop length in beats.
    pub len: f32,
    /// Loop index on the song grid: `floor(song_beat / len)`.
    pub cycle: u32,
    /// Loops played since this track (re)started.
    pub iteration: u32,
    pub seed: u64,
    pub key: Key,
    pub knobs: [f32; 128],
}

impl Default for PhraseCtx {
    fn default() -> Self {
        Self { beat: 0.0, len: 4.0, cycle: 0, iteration: 0, seed: 0, key: Key::new(C4, MAJOR), knobs: [0.0; 128] }
    }
}

/// Arpeggio direction for [`Phrase::arp`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arp {
    Up,
    Down,
    UpDown,
    Random,
}

pub struct Phrase {
    events: Vec<Event>,
    cursor: f32,
    grid: f32,
    vel: f32,
    /// Scale-degree offset applied to degree atoms (`p.root`).
    shift: i32,
    rng: XorShift32,
    key: Key,
    knobs: [f32; 128],
    /// Song beat at the start of this loop.
    pub beat: f64,
    /// Loop length in beats.
    pub len: f32,
    /// Loop index on the song grid.
    pub cycle: u32,
    /// Loops since this track (re)started.
    pub iteration: u32,
}

impl Default for Phrase {
    fn default() -> Self {
        Self::new(PhraseCtx::default())
    }
}

impl Phrase {
    pub fn new(ctx: PhraseCtx) -> Self {
        Self {
            events: Vec::new(),
            cursor: 0.0,
            grid: N16,
            vel: DEFAULT_VEL,
            shift: 0,
            rng: XorShift32::new(ctx.seed),
            key: ctx.key,
            knobs: ctx.knobs,
            beat: ctx.beat,
            len: ctx.len,
            cycle: ctx.cycle,
            iteration: ctx.iteration,
        }
    }

    // ── Cursor, grid, defaults ──

    /// Move the cursor to a loop-local beat.
    pub fn at(&mut self, beat: f32) -> &mut Self {
        self.cursor = beat;
        self
    }

    /// Advance the cursor without placing anything.
    pub fn rest(&mut self, dur: f32) -> &mut Self {
        self.cursor += dur;
        self
    }

    /// Step length for mini-notation strings (default a 16th).
    pub fn grid(&mut self, step: f32) -> &mut Self {
        self.grid = step;
        self
    }

    /// Default velocity for notes placed after this call.
    pub fn vel(&mut self, v: f32) -> &mut Self {
        self.vel = v;
        self
    }

    /// Make degree atoms relative to scale degree `d`: after `p.root(5)`,
    /// `"1"` is the fifth. Handy for following a chord progression.
    pub fn root(&mut self, d: i32) -> &mut Self {
        self.shift = d - 1;
        self
    }

    /// The key degrees resolve against.
    pub fn key(&self) -> Key {
        self.key
    }

    /// Change the key for the rest of this phrase.
    pub fn set_key(&mut self, key: Key) -> &mut Self {
        self.key = key;
        self
    }

    // ── Placement ──

    fn placed(&mut self, first: usize, start: f32, advance: f32) -> Placed<'_> {
        let end = self.events.len();
        Placed { phrase: self, range: first..end, start, advance }
    }

    /// Place a note at the cursor and advance by `dur`.
    pub fn note(&mut self, note: u8, dur: f32) -> Placed<'_> {
        self.chord(&[note], dur)
    }

    /// Place scale degree `d` (in the phrase key) at the cursor.
    pub fn deg(&mut self, d: i32, dur: f32) -> Placed<'_> {
        let n = self.key.deg(d + self.shift);
        self.chord(&[n], dur)
    }

    /// Place every note of a chord at the cursor and advance by `dur`.
    pub fn chord(&mut self, notes: &[u8], dur: f32) -> Placed<'_> {
        let (start, first, vel) = (self.cursor, self.events.len(), self.vel);
        for &n in notes {
            self.events.push(Event::new(start, n, vel, dur));
        }
        self.cursor = start + dur;
        self.placed(first, start, dur)
    }

    /// Mini-notation over scale degrees / note names, one token per grid step:
    /// `p.seq("1 . b3 5 [8 7] 5 _ .")`.
    pub fn seq(&mut self, src: &str) -> Placed<'_> {
        let (key, shift) = (self.key, self.shift);
        self.place_mini(src, Mode::Words, self.grid, |atom| key.resolve(atom, shift).map(|n| (vec![n], None)))
    }

    /// A drum grid for one note: `x` hits, `X` accents, `.` rests —
    /// `p.hits(kick, "x... x... x.x. x...")`.
    pub fn hits(&mut self, note: u8, src: &str) -> Placed<'_> {
        self.place_mini(src, Mode::Chars, self.grid, |atom| {
            Some((vec![note], if atom == "X" { Some(ACCENT_VEL) } else { None }))
        })
    }

    /// Roman-numeral chords in the phrase key, one per `step`:
    /// `p.chords("i VI III VII", bars(1.0))`.
    pub fn chords(&mut self, src: &str, step: f32) -> Placed<'_> {
        let key = self.key;
        self.place_mini(src, Mode::Words, step, |atom| key.roman(atom).map(|ns| (ns, None)))
    }

    /// Arpeggiate a roman-numeral chord at `step` for `len` beats.
    pub fn arp(&mut self, chord: &str, mode: Arp, step: f32, len: f32) -> Placed<'_> {
        let (start, first, vel) = (self.cursor, self.events.len(), self.vel);
        let Some(notes) = self.key.roman(chord) else {
            tracing::warn!("arp: `{chord}` is not a roman-numeral chord");
            return self.placed(first, start, 0.0);
        };
        let order: Vec<u8> = match mode {
            Arp::Up | Arp::Random => notes.clone(),
            Arp::Down => notes.iter().rev().copied().collect(),
            Arp::UpDown => {
                let mut v = notes.clone();
                v.extend(notes.iter().rev().skip(1).take(notes.len().saturating_sub(2)));
                v
            }
        };
        let count = if step > 0.0 { (len / step).round() as usize } else { 0 };
        for i in 0..count {
            let n = match mode {
                Arp::Random => notes[(self.rng.next_u32() as usize) % notes.len()],
                _ => order[i % order.len()],
            };
            self.events.push(Event::new(start + i as f32 * step, n, vel, step * GATE));
        }
        self.cursor = start + len;
        self.placed(first, start, len)
    }

    /// A Euclidean rhythm: `k` hits spread over `n` grid steps.
    pub fn euclid(&mut self, note: u8, k: usize, n: usize) -> Placed<'_> {
        let (start, first, vel, grid) = (self.cursor, self.events.len(), self.vel, self.grid);
        for (i, hit) in euclidean(k, n).into_iter().enumerate() {
            if hit {
                self.events.push(Event::new(start + i as f32 * grid, note, vel, grid * GATE));
            }
        }
        self.cursor = start + n as f32 * grid;
        self.placed(first, start, n as f32 * grid)
    }

    /// Parse and place a mini-notation string. `resolve` maps an atom to notes
    /// plus an optional velocity override; unresolvable atoms are skipped.
    fn place_mini(
        &mut self,
        src: &str,
        mode: Mode,
        step: f32,
        resolve: impl Fn(&str) -> Option<(Vec<u8>, Option<f32>)>,
    ) -> Placed<'_> {
        let (start, first, vel) = (self.cursor, self.events.len(), self.vel);
        let pattern = match mini::parse(src, mode) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!("pattern `{src}`: {e}");
                return self.placed(first, start, 0.0);
            }
        };
        let rng = &mut self.rng;
        let hits = pattern.hits(self.cycle, &mut || rng.next_f32() < 0.5);
        for h in hits {
            let Some((notes, v)) = resolve(&h.atom) else {
                tracing::warn!("pattern `{src}`: can't read `{}`", h.atom);
                continue;
            };
            for n in notes {
                self.events.push(Event::new(start + h.start * step, n, v.unwrap_or(vel), h.dur * step * GATE));
            }
        }
        let advance = pattern.steps() as f32 * step;
        self.cursor = start + advance;
        self.placed(first, start, advance)
    }

    // ── Song position and control flow ──

    /// Song position (bars) at the start of this loop.
    pub fn bar(&self) -> f64 {
        self.beat / 4.0
    }

    pub fn during(&self, s: Section) -> bool {
        s.contains_beat(self.beat)
    }

    pub fn after(&self, s: Section) -> bool {
        self.beat >= s.start_beat()
    }

    /// `0..1` progress through `s` at the start of this loop.
    pub fn progress(&self, s: Section) -> f32 {
        s.progress(self.beat)
    }

    /// Evaluate a signal at the cursor's song position.
    pub fn value(&self, s: &Signal) -> f32 {
        eval_at(s, self.beat + self.cursor as f64, &self.knobs)
    }

    /// Run `f` on every `n`-th loop (the n-th, 2n-th, … — fills land at the
    /// end of each n-loop phrase).
    pub fn every(&mut self, n: u32, f: impl FnOnce(&mut Phrase)) -> &mut Self {
        if n > 0 && self.cycle % n == n - 1 {
            f(self);
        }
        self
    }

    /// Run `f` with probability `prob` (seeded).
    pub fn sometimes(&mut self, prob: f32, f: impl FnOnce(&mut Phrase)) -> &mut Self {
        if self.chance(prob) {
            f(self);
        }
        self
    }

    // ── Whole-phrase transforms ──

    /// Drop each placed note with probability `prob`.
    pub fn degrade(&mut self, prob: f32) -> &mut Self {
        let rng = &mut self.rng;
        self.events.retain(|_| rng.next_f32() >= prob);
        self
    }

    /// Nudge timing by up to `±time` beats and velocity by up to `±vel`.
    pub fn humanize(&mut self, time: f32, vel: f32) -> &mut Self {
        for e in &mut self.events {
            e.beat = (e.beat + (self.rng.next_f32() * 2.0 - 1.0) * time).max(0.0);
            e.vel = (e.vel + (self.rng.next_f32() * 2.0 - 1.0) * vel).clamp(0.0, 1.0);
        }
        self
    }

    /// Rotate everything later by `beats`, wrapping at the loop length.
    pub fn rotate(&mut self, beats: f32) -> &mut Self {
        let len = self.len;
        for e in &mut self.events {
            e.beat = (e.beat + beats).rem_euclid(len);
        }
        self
    }

    /// Play the loop backwards.
    pub fn rev(&mut self) -> &mut Self {
        let len = self.len;
        for e in &mut self.events {
            e.beat = (len - e.beat - e.dur).max(0.0);
        }
        self
    }

    /// Transpose everything placed so far by `semis` semitones.
    pub fn transpose(&mut self, semis: i32) -> &mut Self {
        for e in &mut self.events {
            e.note = (e.note as i32 + semis).clamp(0, 127) as u8;
        }
        self
    }

    // ── Seeded randomness ──

    /// A random `f32` in `range`, reproducible for this cycle.
    pub fn rand(&mut self, range: Range<f32>) -> f32 {
        range.start + (range.end - range.start) * self.rng.next_f32()
    }

    /// Pick one element uniformly at random.
    pub fn pick<T: Copy>(&mut self, xs: &[T]) -> T {
        xs[(self.rng.next_u32() as usize) % xs.len()]
    }

    /// `true` with probability `p`.
    pub fn chance(&mut self, p: f32) -> bool {
        self.rng.next_f32() < p
    }

    /// Finish: sort by time and hold each sliding note's gate into the next.
    pub fn into_events(mut self) -> Vec<Event> {
        self.events.sort_by(|a, b| a.beat.total_cmp(&b.beat));
        for i in 0..self.events.len() {
            if !self.events[i].slide {
                continue;
            }
            let b = self.events[i].beat;
            if let Some(next) = self.events[i + 1..].iter().find(|e| e.beat > b + 1e-4) {
                self.events[i].dur = next.beat - b + SLIDE_OVERLAP;
            }
        }
        self.events
    }
}

fn eval_at(s: &Signal, beat: f64, knobs: &[f32; 128]) -> f32 {
    Program::global(s).eval(&EvalCtx { beat, knobs, voice: &VoiceInputs::default() })
}

// ── Lanes ──

/// A per-note value stream for a [`Placed`] lane: a number, a signal sampled at
/// each note's song position, or a lane string.
///
/// Lane strings containing digits are whitespace-separated numbers (`"800 1.2k
/// . 400"`, where `.`/`~` leave a note alone); otherwise each character is one
/// note (`"X..x g"` for velocity: `X` accent, `x` normal, `g` ghost; `"..x."`
/// for slides). Values cycle over the placed notes; notes sharing an onset (a
/// chord) share a value.
pub enum Lane<'a> {
    Const(f32),
    Text(&'a str),
    Signal(Signal),
}

impl From<f32> for Lane<'_> {
    fn from(v: f32) -> Self {
        Lane::Const(v)
    }
}

impl<'a> From<&'a str> for Lane<'a> {
    fn from(s: &'a str) -> Self {
        Lane::Text(s)
    }
}

impl<'a> From<&'a String> for Lane<'a> {
    fn from(s: &'a String) -> Self {
        Lane::Text(s)
    }
}

impl From<Signal> for Lane<'_> {
    fn from(s: Signal) -> Self {
        Lane::Signal(s)
    }
}

#[derive(Clone, Copy)]
enum LaneKind {
    Vel,
    Slide,
    Number,
}

fn parse_number(tok: &str) -> Option<f32> {
    match tok {
        "." | "~" | "-" | "_" => None,
        _ => match tok.strip_suffix('k') {
            Some(k) => k.parse::<f32>().ok().map(|v| v * 1000.0),
            None => tok.parse().ok(),
        },
    }
}

/// A guard over the notes just placed. Lanes set per-note values; `.step` and
/// `.at` adjust the cursor; `.voice_lead`/`.spread` re-voice placed chords.
pub struct Placed<'a> {
    phrase: &'a mut Phrase,
    range: Range<usize>,
    start: f32,
    advance: f32,
}

impl Placed<'_> {
    /// Index ranges of the placed notes grouped by onset.
    fn groups(&self) -> Vec<Range<usize>> {
        let evs = &self.phrase.events;
        let mut out: Vec<Range<usize>> = Vec::new();
        for i in self.range.clone() {
            match out.last_mut() {
                Some(g) if (evs[g.start].beat - evs[i].beat).abs() < 1e-5 => g.end = i + 1,
                _ => out.push(i..i + 1),
            }
        }
        out
    }

    fn lane(self, lane: Lane<'_>, kind: LaneKind, mut apply: impl FnMut(&mut Event, f32)) -> Self {
        let groups = self.groups();
        let default_vel = self.phrase.vel;
        let values: Vec<Option<f32>> = match lane {
            Lane::Const(v) => vec![Some(v)],
            Lane::Signal(s) => {
                let program = Program::global(&s);
                groups
                    .iter()
                    .map(|g| {
                        let beat = self.phrase.beat + self.phrase.events[g.start].beat as f64;
                        Some(program.eval(&EvalCtx { beat, knobs: &self.phrase.knobs, voice: &VoiceInputs::default() }))
                    })
                    .collect()
            }
            Lane::Text(text) => {
                if text.chars().any(|c| c.is_ascii_digit()) {
                    text.split_whitespace().filter(|t| *t != "|").map(parse_number).collect()
                } else {
                    text.chars()
                        .filter(|c| !c.is_whitespace() && *c != '|')
                        .map(|c| match (kind, c) {
                            (LaneKind::Vel, 'X') => Some(ACCENT_VEL),
                            (LaneKind::Vel, 'x') => Some(default_vel),
                            (LaneKind::Vel, 'g') => Some(GHOST_VEL),
                            (LaneKind::Slide, 'x' | 'X') => Some(1.0),
                            (LaneKind::Slide, _) => Some(0.0),
                            _ => None,
                        })
                        .collect()
                }
            }
        };
        if values.is_empty() {
            return self;
        }
        for (i, g) in groups.into_iter().enumerate() {
            if let Some(v) = values[i % values.len()] {
                for e in &mut self.phrase.events[g] {
                    apply(e, v);
                }
            }
        }
        self
    }

    /// Velocity per note.
    pub fn vel<'l>(self, lane: impl Into<Lane<'l>>) -> Self {
        self.lane(lane.into(), LaneKind::Vel, |e, v| e.vel = v.clamp(0.0, 1.0))
    }

    /// Slide into the next note (`"..x."`; `x` slides).
    pub fn slide<'l>(self, lane: impl Into<Lane<'l>>) -> Self {
        self.lane(lane.into(), LaneKind::Slide, |e, v| e.slide = v > 0.5)
    }

    /// Note length in beats.
    pub fn dur<'l>(self, lane: impl Into<Lane<'l>>) -> Self {
        self.lane(lane.into(), LaneKind::Number, |e, v| e.dur = v.max(0.0))
    }

    /// Lock the filter cutoff (Hz) for each note.
    pub fn cutoff<'l>(self, lane: impl Into<Lane<'l>>) -> Self {
        self.lane(lane.into(), LaneKind::Number, |e, v| e.locks.cutoff = Some(v))
    }

    /// Lock the filter resonance for each note.
    pub fn res<'l>(self, lane: impl Into<Lane<'l>>) -> Self {
        self.lane(lane.into(), LaneKind::Number, |e, v| e.locks.res = Some(v))
    }

    /// Lock the amp envelope decay (seconds) for each note.
    pub fn decay<'l>(self, lane: impl Into<Lane<'l>>) -> Self {
        self.lane(lane.into(), LaneKind::Number, |e, v| e.locks.decay = Some(v))
    }

    /// Lock the voice gain for each note.
    pub fn gain<'l>(self, lane: impl Into<Lane<'l>>) -> Self {
        self.lane(lane.into(), LaneKind::Number, |e, v| e.locks.gain = Some(v))
    }

    /// Override how far the cursor advanced: it moves to `start + adv`.
    pub fn step(mut self, adv: f32) -> Self {
        self.advance = adv;
        self.phrase.cursor = self.start + adv;
        self
    }

    /// Move the placed notes to loop-local `beat`; the cursor follows.
    pub fn at(mut self, beat: f32) -> Self {
        let delta = beat - self.start;
        for e in &mut self.phrase.events[self.range.clone()] {
            e.beat += delta;
        }
        self.start = beat;
        self.phrase.cursor = beat + self.advance;
        self
    }

    /// Transpose the placed notes by `semis`.
    pub fn transpose(self, semis: i32) -> Self {
        for e in &mut self.phrase.events[self.range.clone()] {
            e.note = (e.note as i32 + semis).clamp(0, 127) as u8;
        }
        self
    }

    /// Re-voice each placed chord to move as little as possible from the one
    /// before it — smooth pads from plain roman numerals.
    pub fn voice_lead(mut self) -> Self {
        let mut prev: Vec<u8> = Vec::new();
        for g in self.groups() {
            let notes: Vec<u8> = self.phrase.events[g.clone()].iter().map(|e| e.note).collect();
            let led = voice_lead(&prev, &notes);
            self.write_notes(g, &led);
            prev = led;
        }
        self
    }

    /// Open each placed chord: every other note up `octaves` octaves.
    pub fn spread(mut self, octaves: u8) -> Self {
        for g in self.groups() {
            let notes: Vec<u8> = self.phrase.events[g.clone()].iter().map(|e| e.note).collect();
            self.write_notes(g, &spread(&notes, octaves));
        }
        self
    }

    fn write_notes(&mut self, g: Range<usize>, notes: &[u8]) {
        for (e, &n) in self.phrase.events[g].iter_mut().zip(notes) {
            e.note = n;
        }
    }
}

// ── Euclidean rhythms ──

/// A Euclidean rhythm: `hits` onsets spread as evenly as possible over
/// `steps` slots, starting on the downbeat (Bresenham form: slot `i` hits when
/// `i·hits mod steps < hits`).
pub fn euclidean(hits: usize, steps: usize) -> Vec<bool> {
    let hits = hits.min(steps);
    (0..steps).map(|i| (i * hits) % steps < hits).collect()
}

/// Tiny xorshift32 PRNG, seeded per track and cycle so patterns re-roll each
/// loop yet render reproducibly.
struct XorShift32 {
    state: u32,
}

impl XorShift32 {
    fn new(seed: u64) -> Self {
        let s = (seed ^ (seed >> 32)) as u32;
        Self { state: s | 1 }
    }

    fn next_u32(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        x
    }

    fn next_f32(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::music::duration::{DurExt, N4, N8};
    use crate::music::harmony::MINOR;
    use crate::music::notes::*;
    use crate::music::signal::saw;

    fn events(build: impl FnOnce(&mut Phrase)) -> Vec<Event> {
        let mut p = Phrase::default();
        build(&mut p);
        p.into_events()
    }

    fn beats(evs: &[Event]) -> Vec<f32> {
        evs.iter().map(|e| e.beat).collect()
    }

    fn notes(evs: &[Event]) -> Vec<u8> {
        evs.iter().map(|e| e.note).collect()
    }

    #[test]
    fn note_advances_the_cursor() {
        let evs = events(|p| {
            p.note(60, N16);
            p.note(62, N16);
            p.note(64, N16);
        });
        assert_eq!(beats(&evs), vec![0.0, 0.25, 0.5]);
        assert!(evs.iter().all(|e| e.vel == DEFAULT_VEL));
    }

    #[test]
    fn at_rest_step_and_per_note_vel() {
        let evs = events(|p| {
            p.rest(1.0);
            p.note(60, N8).vel(0.3).step(N8.dotted());
            p.note(62, N8);
            p.at(3.0).note(64, 0.5);
        });
        assert_eq!(beats(&evs), vec![1.0, 1.75, 3.0]);
        assert_eq!(evs[0].vel, 0.3);
    }

    #[test]
    fn seq_resolves_degrees_on_the_grid() {
        let evs = events(|p| {
            p.seq("1 . b3 5");
            p.seq("8"); // cursor advanced by four 16ths
        });
        assert_eq!(beats(&evs), vec![0.0, 0.5, 0.75, 1.0]);
        assert_eq!(notes(&evs), vec![C4, DS4, G4, C5]);
        assert!((evs[0].dur - N16 * GATE).abs() < 1e-6);
    }

    #[test]
    fn grid_and_root_shift() {
        let evs = events(|p| {
            p.grid(N4).root(5).seq("1 3");
        });
        assert_eq!(beats(&evs), vec![0.0, 1.0]);
        assert_eq!(notes(&evs), vec![G4, B4]);
    }

    #[test]
    fn hits_accent_and_advance() {
        let evs = events(|p| {
            p.hits(36, "x.X.");
            p.note(60, N16);
        });
        assert_eq!(beats(&evs), vec![0.0, 0.5, 1.0]);
        assert!(evs[1].vel > evs[0].vel);
    }

    #[test]
    fn alternation_reads_the_cycle() {
        let mut p = Phrase::new(PhraseCtx { cycle: 1, ..Default::default() });
        p.seq("<1 5>");
        assert_eq!(notes(&p.into_events()), vec![G4]);
    }

    #[test]
    fn roman_chords_voice_lead_and_spread() {
        let mut p = Phrase::new(PhraseCtx { key: Key::new(A2, MINOR), ..Default::default() });
        p.chords("i VI", 4.0).voice_lead();
        let evs = p.into_events();
        assert_eq!(beats(&evs), vec![0.0, 0.0, 0.0, 4.0, 4.0, 4.0]);
        // i = A C E; VI (F A C) voice-led keeps A and C, moves E→F.
        let mut second = notes(&evs[3..]);
        second.sort();
        assert_eq!(second, vec![A2, C3, F3]);
        let evs = events(|p| {
            p.chord(&[C4, E4, G4], 1.0).spread(1);
        });
        assert_eq!(notes(&evs), vec![C4, G4, E5]);
    }

    #[test]
    fn lanes_text_numbers_and_signals() {
        let evs = events(|p| {
            p.hits(36, "xxxx").vel("X.g.");
        });
        assert_eq!(evs.iter().map(|e| e.vel).collect::<Vec<_>>(), vec![ACCENT_VEL, DEFAULT_VEL, GHOST_VEL, DEFAULT_VEL]);

        let evs = events(|p| {
            p.hits(36, "xxx").cutoff("800 1.2k .");
        });
        assert_eq!(evs[0].locks.cutoff, Some(800.0));
        assert_eq!(evs[1].locks.cutoff, Some(1200.0));
        assert_eq!(evs[2].locks.cutoff, None);

        let evs = events(|p| {
            p.grid(1.0).hits(36, "xxxx").vel(saw(4.0));
        });
        assert_eq!(evs.iter().map(|e| e.vel).collect::<Vec<_>>(), vec![0.0, 0.25, 0.5, 0.75]);
    }

    #[test]
    fn chord_notes_share_a_lane_step() {
        let evs = events(|p| {
            p.seq("[1,3] 5").vel("0.2 0.9");
        });
        assert_eq!(evs.iter().map(|e| e.vel).collect::<Vec<_>>(), vec![0.2, 0.2, 0.9]);
    }

    #[test]
    fn slides_hold_the_gate_into_the_next_note() {
        let evs = events(|p| {
            p.seq("1 5 8").slide("x..");
        });
        assert!(evs[0].slide && !evs[1].slide);
        assert!((evs[0].dur - (0.25 + SLIDE_OVERLAP)).abs() < 1e-6, "gate overlaps the next onset");
    }

    #[test]
    fn form_queries_and_every() {
        const DROP: Section = Section::at(4, 4);
        let p = Phrase::new(PhraseCtx { beat: 20.0, ..Default::default() });
        assert!(p.during(DROP) && p.after(DROP));
        assert_eq!(p.progress(DROP), 0.25);

        let run = |cycle| {
            let mut p = Phrase::new(PhraseCtx { cycle, ..Default::default() });
            p.every(4, |p| {
                p.note(60, 1.0);
            });
            p.into_events().len()
        };
        assert_eq!((0..8).map(run).collect::<Vec<_>>(), vec![0, 0, 0, 1, 0, 0, 0, 1]);
    }

    #[test]
    fn transforms() {
        let evs = events(|p| {
            p.note(60, 1.0);
            p.transpose(12);
        });
        assert_eq!(evs[0].note, 72);

        let mut p = Phrase::new(PhraseCtx { len: 4.0, ..Default::default() });
        p.at(3.0).note(60, 0.5);
        p.rotate(2.0);
        assert_eq!(p.into_events()[0].beat, 1.0);

        let evs = events(|p| {
            p.euclid(36, 3, 8);
        });
        assert_eq!(beats(&evs), vec![0.0, 0.75, 1.5]);
    }

    #[test]
    fn bad_patterns_place_nothing_and_keep_the_cursor() {
        let evs = events(|p| {
            p.seq("1 [5");
            p.note(60, 1.0);
        });
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].beat, 0.0);
    }

    #[test]
    fn seeded_randomness_is_reproducible_and_rerolls() {
        let build = |seed| {
            let mut p = Phrase::new(PhraseCtx { seed, ..Default::default() });
            for _ in 0..8 {
                let n = p.pick(&[60u8, 62, 64, 65, 67]);
                p.note(n, N8);
            }
            notes(&p.into_events())
        };
        assert_eq!(build(0xABCD), build(0xABCD));
        assert_ne!(build(0x1111), build(0x2222));
    }

    #[test]
    fn euclid_counts() {
        assert_eq!(euclidean(5, 8).iter().filter(|b| **b).count(), 5);
        assert_eq!(euclidean(0, 4), vec![false; 4]);
        assert_eq!(euclidean(4, 4), vec![true; 4]);
    }
}
