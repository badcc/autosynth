use std::ops::Range;

use crate::music::duration::N16;
use crate::music::pattern::Pattern;

/// A single scheduled note: a beat offset, pitch, velocity and duration.
/// The scheduler turns each `NoteSpec` into a note-on plus a note-off
/// obligation, so durations that spill past a loop boundary still release.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NoteSpec {
    /// Beat offset from the start of the phrase (loop-local).
    pub beat: f32,
    pub note: u8,
    pub vel: f32,
    /// Duration in beats.
    pub dur: f32,
}

/// Default velocity for notes placed without an explicit `.vel(..)`.
const DEFAULT_VEL: f32 = 0.8;

/// The one note container. Built inside `every(..)` closures. Notes are placed
/// at a *cursor* that advances as you go, so a run of notes reads as a melody
/// rather than a list of absolute beats. `at`/`rest` move the cursor; per-note
/// tweaks chain off the returned [`Note`] guard.
///
/// Randomness is seeded (from the engine, per iteration) so offline renders stay
/// bit-reproducible — use [`Phrase::rand`]/[`Phrase::pick`]/[`Phrase::chance`]
/// instead of `rand::`.
pub struct Phrase {
    notes: Vec<NoteSpec>,
    /// Beat where the next placement lands. Starts at 0.
    cursor: f32,
    /// Default velocity for placed notes.
    vel: f32,
    rng: XorShift32,
    /// Global beat at the start of this loop iteration.
    pub beat: f32,
    /// Loop iteration count (0 on first play).
    pub iteration: u32,
}

impl Default for Phrase {
    fn default() -> Self {
        Self::new()
    }
}

impl Phrase {
    pub fn new() -> Self {
        Self::with_context(0.0, 0, 0)
    }

    pub(crate) fn with_context(beat: f32, iteration: u32, seed: u64) -> Self {
        Self {
            notes: Vec::new(),
            cursor: 0.0,
            vel: DEFAULT_VEL,
            rng: XorShift32::new(seed),
            beat,
            iteration,
        }
    }

    // ── Cursor placement ──

    /// Move the cursor to an absolute beat.
    pub fn at(&mut self, beat: f32) -> &mut Self {
        self.cursor = beat;
        self
    }

    /// Advance the cursor by `dur` beats without placing a note.
    pub fn rest(&mut self, dur: f32) -> &mut Self {
        self.cursor += dur;
        self
    }

    /// Set the default velocity for notes placed after this call.
    pub fn vel(&mut self, v: f32) -> &mut Self {
        self.vel = v;
        self
    }

    /// Place a note at the cursor and advance the cursor by `dur`.
    pub fn note(&mut self, note: u8, dur: f32) -> Note<'_> {
        let start = self.cursor;
        let idx = self.notes.len();
        let vel = self.vel;
        self.notes.push(NoteSpec { beat: start, note, vel, dur });
        self.cursor = start + dur;
        Note { phrase: self, start, advance: dur, range: idx..idx + 1 }
    }

    /// Place every note of a chord at the cursor, advancing by `dur`.
    pub fn chord(&mut self, notes: &[u8], dur: f32) -> Note<'_> {
        let start = self.cursor;
        let idx = self.notes.len();
        let vel = self.vel;
        for &n in notes {
            self.notes.push(NoteSpec { beat: start, note: n, vel, dur });
        }
        let end = self.notes.len();
        self.cursor = start + dur;
        Note { phrase: self, start, advance: dur, range: idx..end }
    }

    /// Step-sequencer notation from the cursor. Each character is one 16th note:
    /// `x` = hit at the default velocity, `X` = accent, anything else = rest.
    /// Advances the cursor by `len * N16`.
    pub fn steps(&mut self, note: u8, pattern: &str) -> &mut Self {
        let start = self.cursor;
        let dur = N16 * 0.9; // slight gap before the next step
        let mut count = 0;
        for (i, ch) in pattern.chars().enumerate() {
            count = i + 1;
            let v = match ch {
                'x' => self.vel,
                'X' => (self.vel * 1.3).min(1.0),
                _ => continue,
            };
            self.notes.push(NoteSpec { beat: start + i as f32 * N16, note, vel: v, dur });
        }
        self.cursor = start + count as f32 * N16;
        self
    }

    /// Lay a reusable [`Pattern`] down from the cursor, advancing by its length.
    pub fn pattern(&mut self, pattern: &Pattern) -> &mut Self {
        let start = self.cursor;
        for pn in pattern.notes() {
            self.notes.push(NoteSpec {
                beat: start + pn.beat,
                note: pn.note,
                vel: pn.velocity,
                dur: pn.duration,
            });
        }
        self.cursor = start + pattern.length();
        self
    }

    // ── Seeded randomness ──

    /// A random `f32` in `range`. Deterministic for a given seed + iteration.
    pub fn rand(&mut self, range: Range<f32>) -> f32 {
        range.start + (range.end - range.start) * self.rng.next_f32()
    }

    /// Pick one element uniformly at random.
    pub fn pick<T: Copy>(&mut self, xs: &[T]) -> T {
        let i = (self.rng.next_u32() as usize) % xs.len();
        xs[i]
    }

    /// `true` with probability `p`.
    pub fn chance(&mut self, p: f32) -> bool {
        self.rng.next_f32() < p
    }

    pub(crate) fn into_notes(self) -> Vec<NoteSpec> {
        self.notes
    }
}

/// A guard over the note(s) just placed by [`Phrase::note`]/[`Phrase::chord`].
/// Chains per-note tweaks: `.vel` sets velocity, `.at` repositions to an
/// absolute beat, `.step` overrides how far the cursor advanced.
pub struct Note<'a> {
    phrase: &'a mut Phrase,
    /// Beat the notes were placed at.
    start: f32,
    /// How far the cursor advanced past `start`.
    advance: f32,
    range: Range<usize>,
}

impl Note<'_> {
    /// Set the velocity of the just-placed note(s).
    pub fn vel(self, v: f32) -> Self {
        for n in &mut self.phrase.notes[self.range.clone()] {
            n.vel = v;
        }
        self
    }

    /// Reposition the note(s) to an absolute beat; the cursor follows to
    /// `beat + advance`.
    pub fn at(mut self, beat: f32) -> Self {
        let delta = beat - self.start;
        for n in &mut self.phrase.notes[self.range.clone()] {
            n.beat += delta;
        }
        self.start = beat;
        self.phrase.cursor = beat + self.advance;
        self
    }

    /// Override the cursor advance: the cursor moves to `start + adv`.
    pub fn step(mut self, adv: f32) -> Self {
        self.advance = adv;
        self.phrase.cursor = self.start + adv;
        self
    }
}

/// Tiny xorshift32 PRNG. `music` depends on nothing, so it carries its own — the
/// dsp voice PRNG lives a layer below. Seeded from the engine so patterns re-roll
/// each iteration while staying reproducible across renders.
struct XorShift32 {
    state: u32,
}

impl XorShift32 {
    fn new(seed: u64) -> Self {
        // Fold to 32 bits and force nonzero (xorshift is stuck at 0).
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
        // Top 24 bits → [0, 1).
        (self.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::music::duration::{DurExt, N8};

    fn notes(build: impl FnOnce(&mut Phrase)) -> Vec<NoteSpec> {
        let mut p = Phrase::new();
        build(&mut p);
        p.into_notes()
    }

    #[test]
    fn note_advances_the_cursor() {
        let ns = notes(|p| {
            p.note(60, N16);
            p.note(62, N16);
            p.note(64, N16);
        });
        assert_eq!(ns.iter().map(|n| n.beat).collect::<Vec<_>>(), vec![0.0, 0.25, 0.5]);
        assert!(ns.iter().all(|n| n.vel == DEFAULT_VEL));
    }

    #[test]
    fn rest_and_at_move_the_cursor() {
        let ns = notes(|p| {
            p.rest(1.0);
            p.note(60, 0.5);
            p.at(3.0);
            p.note(62, 0.5);
        });
        assert_eq!(ns[0].beat, 1.0);
        assert_eq!(ns[1].beat, 3.0);
    }

    #[test]
    fn per_note_vel_and_step_override() {
        let ns = notes(|p| {
            p.note(60, N8).vel(0.3).step(N8.dotted());
            p.note(62, N8);
        });
        assert_eq!(ns[0].vel, 0.3);
        // step overrode the advance to a dotted eighth (0.75).
        assert_eq!(ns[1].beat, 0.75);
    }

    #[test]
    fn note_at_repositions_and_moves_cursor() {
        let ns = notes(|p| {
            p.note(60, 0.5).at(2.0);
            p.note(62, 0.5);
        });
        assert_eq!(ns[0].beat, 2.0);
        assert_eq!(ns[1].beat, 2.5, "cursor followed to beat + advance");
    }

    #[test]
    fn default_vel_setter() {
        let ns = notes(|p| {
            p.vel(0.5);
            p.note(60, N8);
        });
        assert_eq!(ns[0].vel, 0.5);
    }

    #[test]
    fn steps_place_from_cursor_and_advance() {
        let ns = notes(|p| {
            p.rest(1.0);
            p.steps(36, "x.X.");
            p.note(60, N16);
        });
        // Two hits at beats 1.0 and 1.5 (a full 16th grid from the cursor).
        assert_eq!(ns[0].beat, 1.0);
        assert_eq!(ns[1].beat, 1.5);
        assert!(ns[1].vel > ns[0].vel, "X is an accent");
        // The trailing note lands after four 16ths: 1.0 + 4*0.25 = 2.0.
        assert_eq!(ns[2].beat, 2.0);
    }

    #[test]
    fn same_seed_and_iteration_is_reproducible() {
        let build = |p: &mut Phrase| {
            for _ in 0..8 {
                let n = p.pick(&[60u8, 62, 64, 65, 67]);
                let v = p.rand(0.6..0.8);
                p.note(n, N8).vel(v);
            }
        };
        let mut a = Phrase::with_context(0.0, 3, 0xABCD);
        build(&mut a);
        let mut b = Phrase::with_context(0.0, 3, 0xABCD);
        build(&mut b);
        assert_eq!(a.into_notes(), b.into_notes());
    }

    #[test]
    fn different_iteration_rerolls() {
        let build = |p: &mut Phrase| {
            for _ in 0..8 {
                let n = p.pick(&[60u8, 62, 64, 65, 67]);
                p.note(n, N8);
            }
        };
        // The engine mixes the iteration into the seed; emulate two distinct
        // seeds and require the phrases to differ.
        let mut a = Phrase::with_context(0.0, 0, 0x1111);
        build(&mut a);
        let mut b = Phrase::with_context(0.0, 1, 0x2222);
        build(&mut b);
        assert_ne!(a.into_notes(), b.into_notes());
    }
}
