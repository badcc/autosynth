//! The real-time core. Owns the beat-native transport, per-track schedulers and
//! voice banks, the effect chains, and the master mixer. Renders block-by-block
//! with control-rate automation. Knows nothing about hot-reload or cpal — those
//! live in `live`.

pub mod command;
pub mod instrument;
pub mod mixer;
pub mod scheduler;
pub mod track;
pub mod transport;
pub mod voice;

mod run;

pub use command::{Command, EngineHandle, TrackBuild};
pub use run::Engine;
pub use transport::Transport;

/// Control-period length in frames. Automation is evaluated once per period;
/// smoothed params glide across it. 64 frames ≈ 1.5 ms at 44.1 kHz.
pub const CONTROL_BLOCK: usize = 64;
