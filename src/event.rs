use crate::patch::Patch;

#[derive(Clone, Copy, Debug)]
pub enum Param {
    Attack,
    Decay,
    Sustain,
    Release,
    Cutoff,
    Resonance,
    LfoRate,
    LfoDepth,
    MasterGain,
}

#[derive(Clone, Debug)]
pub enum EventKind {
    NoteOn { note: u8, vel: f32 },
    NoteOff { note: u8 },
    Param { param: Param, value: f32 },
    SetPatch { patch: Patch },
}

#[derive(Clone, Debug)]
pub struct Event {
    pub sample: u64,
    pub kind: EventKind,
}
