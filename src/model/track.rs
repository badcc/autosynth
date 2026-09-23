//! Tracks and buses as data, and the identities that name them.

use std::any::{TypeId, type_name};

use crate::model::chain::ChainSpec;
use crate::model::instrument::InstrumentSpec;
use crate::music::phrase::Phrase;
use crate::music::pitch::Key;
use crate::music::signal::Signal;

macro_rules! fn_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Debug, PartialEq, Eq, Hash)]
        pub struct $name {
            ty: TypeId,
            /// Full module path of the function: the engine key.
            pub key: String,
        }

        impl $name {
            pub fn of<F: 'static>() -> Self {
                Self { ty: TypeId::of::<F>(), key: type_name::<F>().to_string() }
            }

            /// The function's own name, for display.
            pub fn name(&self) -> &str {
                self.key.rsplit("::").find(|s| !s.starts_with('{')).unwrap_or("?")
            }
        }
    };
}

fn_id! {
    /// A track's identity: the type of its builder function. Every function
    /// item has a unique type, so the function *is* the name — stable across
    /// hot reloads, collision-proof across modules.
    TrackId
}

fn_id! {
    /// A bus's identity: the type of its builder function.
    BusId
}

/// Where a track or bus sends its output.
#[derive(Clone, Debug, PartialEq, Default)]
pub enum Route {
    #[default]
    Master,
    Bus(BusId),
}

/// Trigger-based sidechain: every note-on of `source` dips this track's gain by
/// `depth`, recovering over `release` beats.
#[derive(Clone, Debug, PartialEq)]
pub struct DuckSpec {
    pub source: TrackId,
    pub depth: f32,
    pub release: f32,
}

/// Swing: notes on odd multiples of `grid` land late by `(amount - 0.5) * 2 *
/// grid` beats. `0.5` is straight.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Swing {
    pub grid: f32,
    pub amount: f32,
}

/// A looping pattern closure, re-run every loop.
pub type PatternFn = Box<dyn FnMut(&mut Phrase) + Send>;

pub struct Playback {
    pub len: f32,
    pub pattern: PatternFn,
}

/// The complete description of one track for one frame.
pub struct TrackSpec {
    pub instrument: InstrumentSpec,
    pub gain: Signal,
    pub pan: Signal,
    pub mute: bool,
    pub chain: ChainSpec,
    pub route: Route,
    pub duck: Option<DuckSpec>,
    /// Overrides the scene key for this track's patterns.
    pub key: Option<Key>,
    pub swing: Option<Swing>,
    pub playback: Option<Playback>,
}

impl Default for TrackSpec {
    fn default() -> Self {
        Self {
            instrument: InstrumentSpec::default(),
            gain: Signal::constant(1.0),
            pan: Signal::constant(0.0),
            mute: false,
            chain: ChainSpec::default(),
            route: Route::Master,
            duck: None,
            key: None,
            swing: None,
            playback: None,
        }
    }
}

/// A bus: tracks (and other buses) route or send into it; it has its own gain,
/// chain, and route.
#[derive(Clone, Debug, PartialEq)]
pub struct BusSpec {
    pub gain: Signal,
    pub mute: bool,
    pub chain: ChainSpec,
    pub route: Route,
}

impl Default for BusSpec {
    fn default() -> Self {
        Self { gain: Signal::constant(1.0), mute: false, chain: ChainSpec::default(), route: Route::Master }
    }
}
