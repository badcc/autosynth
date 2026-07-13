//! Pure musical vocabulary: notes, harmony, durations, patterns, phrases, and
//! the `Clock` timing context. No engine or DSP knowledge lives here — every
//! item is plain data or a pure function.

pub mod clock;
pub mod duration;
pub mod harmony;
pub mod notes;
pub mod pattern;
pub mod phrase;

pub use clock::Clock;
pub use phrase::{NoteSpec, Phrase};
