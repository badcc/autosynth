use crate::patch::Patch;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SynthParam {
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

#[derive(Clone, Debug, PartialEq)]
pub enum EventKind {
    NoteOn { note: u8, vel: f32 },
    NoteOff { note: u8 },
    Param { param: SynthParam, value: f32 },
    SetPatch { patch: Patch },
}

#[derive(Clone, Debug)]
pub struct Event {
    pub sample: u64,
    pub kind: EventKind,
}
