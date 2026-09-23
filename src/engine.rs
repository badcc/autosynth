//! The real-time core: beat-native transport, per-track schedulers and voices,
//! effect chains, buses and the master. Renders block-by-block with
//! control-rate signal evaluation. Knows nothing about hot reload or cpal —
//! those live in `live`.

pub mod bus;
pub mod chain;
pub mod command;
pub mod instrument;
pub mod mixer;
pub mod run;
pub mod scheduler;
pub mod track;
pub mod transport;

/// Control-period length in frames. Signals are evaluated once per period;
/// smoothed params glide across it. 64 frames ≈ 1.5 ms at 44.1 kHz.
pub const CONTROL_BLOCK: usize = 64;
