//! The declarative, diffable description of a scene. User builder code produces
//! `TrackSpec` values; the live layer diffs them frame-to-frame and turns the
//! differences into engine commands. Everything here is plain data (plus a few
//! boxed closures that can only be compared by presence).

pub mod fx;
pub mod param;
pub mod patch;
pub mod track;

pub use fx::{FxKind, FxSpec};
pub use param::{Automation, AutomationFn, IntoVal, ParamId, PatternFn, Val};
pub use patch::PatchSpec;
pub use track::{SourceSpec, TrackSpec};
