use std::sync::mpsc;

use crate::engine::bus::BusUpdate;
use crate::engine::chain::NodeUpdate;
use crate::engine::instrument::InstrumentCfg;
use crate::engine::track::{DuckCfg, PatternCfg, TrackBuild};
use crate::music::pitch::Key;
use crate::music::signal::Program;

/// Commands from the control thread to the engine. The scene diff turns model
/// changes into these; sound changes apply immediately, patterns queue to the
/// next loop boundary.
pub enum Command {
    AddTrack(Box<TrackBuild>),
    RemoveTrack(String),

    SetInstrument { track: String, cfg: Box<InstrumentCfg> },
    SetMixer { track: String, gain: Program, pan: Program, mute: bool },
    SetChain { track: String, updates: Vec<NodeUpdate> },
    SetRoute { track: String, route: Option<String> },
    SetDuck { track: String, duck: Option<DuckCfg> },
    SetTrackKey { track: String, key: Option<Key> },
    QueuePattern { track: String, pattern: PatternCfg },

    /// The complete bus set, in processing order.
    SetBuses(Vec<BusUpdate>),
    SetMaster(Vec<NodeUpdate>),

    SetTempo(f32),
    SetKey(Key),
    /// Move the song to this song beat at the next bar line.
    Jump(f64),
    /// Loop the song between two song beats, or stop looping.
    Hold(Option<(f64, f64)>),

    MidiNoteOn { note: u8, vel: f32 },
    MidiNoteOff { note: u8 },
    MidiCc { cc: u8, value: f32 },
    SetMidiTrack(Option<String>),
}

/// A cloneable command sender — the engine's only inbound channel.
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
}
