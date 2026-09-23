//! Pure musical vocabulary: notes, durations, keys and harmony, song form,
//! signals, mini-notation and the `Phrase` note container. No engine or DSP
//! knowledge lives here — every item is plain data or a pure function.

pub mod duration;
pub mod form;
pub mod harmony;
pub mod mini;
pub mod notes;
pub mod phrase;
pub mod pitch;
pub mod signal;
