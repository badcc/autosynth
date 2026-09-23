//! The declarative, diffable description of a scene. Builder code produces
//! these values; the live layer diffs them frame-to-frame and turns the
//! differences into engine commands. Everything here is plain data — signals
//! included — except the pattern closure, which is resent whenever its track's
//! builder re-runs.

pub mod chain;
pub mod instrument;
pub mod track;
