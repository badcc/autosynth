use std::sync::mpsc;

use crate::dsp::effects::Effect;
use crate::engine::instrument::SampleSource;
use crate::model::param::{Automation, PatternFn};
use crate::model::PatchSpec;
use crate::music::NoteSpec;

/// The resolved sound source for a new track. Sample data is already loaded
/// (as `Arc`) on the control thread — the audio thread never touches the disk.
pub enum BuiltSource {
    Synth,
    Sample(SampleSource),
}

/// A group bus built on the control thread: which tracks it sums, its gain, and
/// its (already-built) shared fx chain.
pub struct GroupBuild {
    pub members: Vec<String>,
    pub gain: f32,
    pub fx: Vec<Box<dyn Effect>>,
    pub fx_enabled: Vec<bool>,
}

/// What a track plays when created.
pub enum Playback {
    Pattern { func: PatternFn, loop_len: f64 },
    OneShot(Vec<NoteSpec>),
    Silent,
}

/// Everything needed to construct a running track, built on the control thread
/// (fx boxed, samples loaded) and shipped whole.
pub struct TrackBuild {
    pub source: BuiltSource,
    pub patch: PatchSpec,
    pub polyphony: usize,
    pub gain: f32,
    pub pan: f32,
    pub mute: bool,
    pub fx: Vec<Box<dyn Effect>>,
    pub fx_enabled: Vec<bool>,
    pub automations: Vec<Automation>,
    pub playback: Playback,
}

/// Commands sent from the control thread to the engine. One `AddTrack` replaces
/// the old four add-variants; timing and sound changes are separate so the
/// engine can apply them at the right moment.
pub enum Command {
    AddTrack {
        name: String,
        build: Box<TrackBuild>,
    },
    RemoveTrack(String),
    StopTrack(String),

    // Sound-speed updates (applied immediately).
    SetPatch {
        track: String,
        patch: PatchSpec,
    },
    SetMixer {
        track: String,
        gain: f32,
        pan: f32,
        mute: bool,
    },
    SetFx {
        track: String,
        fx: Vec<Box<dyn Effect>>,
        enabled: Vec<bool>,
    },
    SetFxEnabled {
        track: String,
        enabled: Vec<bool>,
    },
    SetAutomations {
        track: String,
        automations: Vec<Automation>,
    },

    // Timing-speed updates (queued to loop boundary).
    QueuePattern {
        track: String,
        func: PatternFn,
        loop_len: f64,
    },
    QueueOneShot {
        track: String,
        notes: Vec<NoteSpec>,
    },

    /// Replace the whole set of group buses. The scene ships the complete set
    /// whenever it changes, so there is no per-group add/remove to reconcile.
    SetGroups(Vec<GroupBuild>),

    // MIDI input.
    MidiNoteOn { note: u8, vel: f32 },
    MidiNoteOff { note: u8 },
    SetMidiTrack(Option<String>),

    SetTempo(f32),
}

/// A cloneable command sender. This is the engine's only inbound channel; the
/// scene diff turns model changes into these.
#[derive(Clone)]
pub struct EngineHandle {
    tx: mpsc::Sender<Command>,
}

impl EngineHandle {
    pub(crate) fn new(tx: mpsc::Sender<Command>) -> Self {
        Self { tx }
    }

    pub fn send(&self, cmd: Command) {
        let _ = self.tx.send(cmd);
    }

    pub fn midi_note_on(&self, note: u8, vel: f32) {
        self.send(Command::MidiNoteOn { note, vel });
    }

    pub fn midi_note_off(&self, note: u8) {
        self.send(Command::MidiNoteOff { note });
    }
}
