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

/// The one note container. Built inside `every(..)` closures and one-shot
/// `note`/`chord`/`pattern` calls. All times are plain beats (`f32`).
pub struct Phrase {
    notes: Vec<NoteSpec>,
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
        Self {
            notes: Vec::new(),
            beat: 0.0,
            iteration: 0,
        }
    }

    pub(crate) fn with_context(beat: f32, iteration: u32) -> Self {
        Self {
            notes: Vec::new(),
            beat,
            iteration,
        }
    }

    pub fn note(&mut self, beat: f32, note: u8, vel: f32, dur: f32) -> &mut Self {
        self.notes.push(NoteSpec {
            beat,
            note,
            vel,
            dur,
        });
        self
    }

    pub fn chord(&mut self, beat: f32, notes: &[u8], vel: f32, dur: f32) -> &mut Self {
        for &n in notes {
            self.note(beat, n, vel, dur);
        }
        self
    }

    pub fn pattern(&mut self, start: f32, pattern: &Pattern) -> &mut Self {
        for pn in pattern.notes() {
            self.note(start + pn.beat, pn.note, pn.velocity, pn.duration);
        }
        self
    }

    /// Step-sequencer notation. Each character is one 16th note (0.25 beats):
    /// `x` = hit at `vel`, `X` = accent, anything else = rest.
    pub fn steps(&mut self, note: u8, pattern: &str, vel: f32) -> &mut Self {
        let step = 0.25_f32;
        let dur = 0.225_f32; // slight gap before the next step
        for (i, ch) in pattern.chars().enumerate() {
            let v = match ch {
                'x' => vel,
                'X' => (vel * 1.3).min(1.0),
                _ => continue,
            };
            self.note(i as f32 * step, note, v, dur);
        }
        self
    }

    pub(crate) fn into_notes(self) -> Vec<NoteSpec> {
        self.notes
    }
}
