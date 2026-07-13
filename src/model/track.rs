use std::path::PathBuf;

use crate::model::fx::FxSpec;
use crate::model::param::{Automation, PatternFn};
use crate::model::patch::PatchSpec;
use crate::music::NoteSpec;

/// Where a track's sound comes from. Compared by value on hot-reload — a change
/// here (different sample, different kit) forces a full rebuild of the track's
/// voices.
#[derive(Clone, Debug, PartialEq)]
pub enum SourceSpec {
    /// Subtractive synth using `PatchSpec::oscillators`.
    Synth,
    /// Pitched one-shot sample, `root` mapping the file's natural pitch.
    Sample { path: PathBuf, root: u8 },
    /// Drum kit: each MIDI note plays a different sample.
    Kit { slots: Vec<(u8, PathBuf)> },
}

/// Swing: shift every note that lands on an *odd* multiple of `grid` late by
/// `(amount - 0.5) * 2 * grid` beats. `amount == 0.5` is straight timing. A
/// *timing* property — it rides the pattern/one-shot payloads, not the diff.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Swing {
    pub grid: f32,
    pub amount: f32,
}

/// The complete, diffable description of one track for a single frame.
///
/// The diffable fields (`source`, `patch`, `gain`, `pan`, `mute`, `fx`,
/// `polyphony`, `loop_len`) drive hot-reload decisions. The closure/data fields
/// (`pattern`, `one_shot`, `automations`, `swing`) can't be value-compared (or
/// are timing-scoped), so they are resent whenever the builder re-runs.
pub struct TrackSpec {
    pub source: SourceSpec,
    pub patch: PatchSpec,
    pub gain: f32,
    pub pan: f32,
    pub mute: bool,
    pub fx: Vec<FxSpec>,
    pub polyphony: usize,
    pub loop_len: Option<f32>,
    pub pattern: Option<PatternFn>,
    pub one_shot: Vec<NoteSpec>,
    pub automations: Vec<Automation>,
    pub swing: Option<Swing>,
}

impl Default for TrackSpec {
    fn default() -> Self {
        Self {
            source: SourceSpec::Synth,
            patch: PatchSpec::default(),
            gain: 1.0,
            pan: 0.0,
            mute: false,
            fx: Vec::new(),
            polyphony: 8,
            loop_len: None,
            pattern: None,
            one_shot: Vec::new(),
            automations: Vec::new(),
            swing: None,
        }
    }
}

impl TrackSpec {
    /// Whether the diffable *sound* fields (everything but timing) changed.
    pub fn sound_differs(&self, other: &TrackSpec) -> bool {
        self.patch != other.patch
            || self.gain != other.gain
            || self.pan != other.pan
            || self.mute != other.mute
    }

    /// Whether the fx chain configuration changed (kind/params/enabled).
    pub fn fx_differs(&self, other: &TrackSpec) -> bool {
        self.fx != other.fx
    }

    /// Whether the source type changed (requires rebuilding voices).
    pub fn source_differs(&self, other: &TrackSpec) -> bool {
        self.source != other.source || self.polyphony != other.polyphony
    }

    /// Whether the loop length changed (a timing change).
    pub fn loop_differs(&self, other: &TrackSpec) -> bool {
        self.loop_len != other.loop_len
    }
}
