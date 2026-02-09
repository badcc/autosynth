use std::any::TypeId;
use std::collections::{HashMap, HashSet};

use subsecond::{HotFn, HotFnPtr};

use crate::diff;
use crate::effect_config::EffectConfig;
use crate::engine::EngineHandle;
use crate::envelope::RetriggerMode;
use crate::event::EventKind;
use crate::filter::FilterType;
use crate::oscillator::Oscillator;
use crate::patch::Patch;
use crate::pattern::Pattern;
use crate::score::{Tempo, Time};
use crate::waveform::Waveform;

// ── Identity ──

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TrackId(TypeId);

impl TrackId {
    pub fn of<F: 'static>() -> Self {
        Self(TypeId::of::<F>())
    }
}

/// Extract short function name from type_name (e.g. "simple::chords" → "chords")
fn short_name<F: 'static>() -> String {
    let full = std::any::type_name::<F>();
    full.rsplit("::")
        .find(|s| !s.starts_with('{'))
        .unwrap_or("track")
        .to_string()
}

// ── Description types (diffable snapshots) ──

#[derive(Clone, Debug, PartialEq)]
pub struct TrackDesc {
    pub patch: Patch,
    pub polyphony: usize,
    pub effects: Vec<EffectConfig>,
    pub events: Vec<(Time, EventKind)>,
    pub loop_beats: Option<f32>,
}

impl Default for TrackDesc {
    fn default() -> Self {
        Self {
            patch: Patch::new(),
            polyphony: 8,
            effects: Vec::new(),
            events: Vec::new(),
            loop_beats: None,
        }
    }
}

// ── Scene (stateful, long-lived) ──

pub struct Scene {
    bpm: f32,
    sample_rate: f32,
    handle: EngineHandle,
    tracks: HashMap<TrackId, (String, TrackDesc)>,
    ptrs: HashMap<TrackId, HotFnPtr>,
    seen: HashSet<TrackId>,
}

impl Scene {
    pub fn new(bpm: f32, sample_rate: f32, handle: EngineHandle) -> Self {
        Self {
            bpm,
            sample_rate,
            handle,
            tracks: HashMap::new(),
            ptrs: HashMap::new(),
            seen: HashSet::new(),
        }
    }

    pub fn tempo(&mut self, bpm: f32) {
        if self.bpm != bpm {
            self.bpm = bpm;
            self.handle.set_tempo(Tempo::new(bpm));
        }
    }

    /// Register a track. The function IS the identity.
    /// F is typically a named fn item like `chords` — its TypeId is stable.
    /// Uses HotFn ptr_address to skip unchanged tracks between patches.
    pub fn track<F: Fn(&mut SceneTrack) + 'static>(&mut self, f: F) {
        let id = TrackId::of::<F>();
        self.seen.insert(id);

        // Check if this track's function pointer changed
        let mut hot = HotFn::current(f);
        let ptr = hot.ptr_address();

        if let Some(old_ptr) = self.ptrs.get(&id) {
            if *old_ptr == ptr {
                // Function unchanged — skip entirely
                return;
            }
        }

        // Function changed (or new track) — evaluate it
        self.ptrs.insert(id, ptr);

        let name = short_name::<F>();
        let mut builder = SceneTrack::new();
        hot.call((&mut builder,));
        let new_desc = builder.into_desc();

        if let Some((_, old_desc)) = self.tracks.get(&id) {
            // Existing track — diff
            diff::diff_track(&self.handle, &name, old_desc, &new_desc, self.bpm, self.sample_rate);
        } else {
            // New track — create from scratch
            diff::add_track(&self.handle, &name, &new_desc, self.bpm, self.sample_rate);
        }

        self.tracks.insert(id, (name, new_desc));
    }

    /// Call after all track() calls in a frame to detect removed tracks.
    pub fn finish_frame(&mut self) {
        // Remove tracks not seen this frame
        let removed: Vec<TrackId> = self
            .tracks
            .keys()
            .filter(|id| !self.seen.contains(id))
            .copied()
            .collect();

        for id in removed {
            if let Some((name, _)) = self.tracks.remove(&id) {
                self.handle.remove_track(&name);
            }
            self.ptrs.remove(&id);
        }

        self.seen.clear();
    }
}

// ── Track builder (user-facing, re-exported as `Track` in prelude) ──

pub struct SceneTrack {
    desc: TrackDesc,
    oscs_set: bool,
}

impl SceneTrack {
    fn new() -> Self {
        Self {
            desc: TrackDesc::default(),
            oscs_set: false,
        }
    }

    fn into_desc(self) -> TrackDesc {
        self.desc
    }

    // ── Oscillator config ──

    /// Add an oscillator. First call clears the default oscillators.
    /// Returns &mut Oscillator for chaining (.set_detune(), .set_phase_offset()).
    pub fn osc(&mut self, waveform: Waveform, level: f32) -> &mut Oscillator {
        if !self.oscs_set {
            self.desc.patch.oscillators.clear();
            self.oscs_set = true;
        }
        self.desc
            .patch
            .oscillators
            .push(Oscillator::new(waveform).level(level));
        self.desc.patch.oscillators.last_mut().unwrap()
    }

    // ── Patch parameters (direct setters) ──

    pub fn gain(&mut self, v: f32) {
        self.desc.patch.master_gain = v;
    }

    pub fn attack(&mut self, v: f32) {
        self.desc.patch.attack = v;
    }

    pub fn decay(&mut self, v: f32) {
        self.desc.patch.decay = v;
    }

    pub fn sustain(&mut self, v: f32) {
        self.desc.patch.sustain = v;
    }

    pub fn release(&mut self, v: f32) {
        self.desc.patch.release = v;
    }

    pub fn cutoff(&mut self, v: f32) {
        self.desc.patch.cutoff = v;
    }

    pub fn resonance(&mut self, v: f32) {
        self.desc.patch.resonance = v;
    }

    pub fn lfo_rate(&mut self, v: f32) {
        self.desc.patch.lfo_rate = v;
    }

    pub fn lfo_depth(&mut self, v: f32) {
        self.desc.patch.lfo_depth = v;
    }

    pub fn filter_type(&mut self, v: FilterType) {
        self.desc.patch.filter_type = v;
    }

    pub fn retrigger(&mut self, v: RetriggerMode) {
        self.desc.patch.retrigger = v;
    }

    pub fn polyphony(&mut self, n: usize) {
        self.desc.polyphony = n;
    }

    // ── Effects ──

    pub fn delay(&mut self, f: impl FnOnce(&mut crate::effect_config::DelayBuilder)) {
        let mut b = crate::effect_config::DelayBuilder::new();
        f(&mut b);
        self.desc.effects.push(EffectConfig::Delay(b.into_config()));
    }

    pub fn distortion(&mut self, f: impl FnOnce(&mut crate::effect_config::DistortionBuilder)) {
        let mut b = crate::effect_config::DistortionBuilder::new();
        f(&mut b);
        self.desc
            .effects
            .push(EffectConfig::Distortion(b.into_config()));
    }

    pub fn chorus(&mut self, f: impl FnOnce(&mut crate::effect_config::ChorusBuilder)) {
        let mut b = crate::effect_config::ChorusBuilder::new();
        f(&mut b);
        self.desc
            .effects
            .push(EffectConfig::Chorus(b.into_config()));
    }

    // ── Clip / notes ──

    /// Set loop length in beats. Notes will loop every N beats.
    pub fn every(&mut self, beats: f32) {
        self.desc.loop_beats = Some(beats);
    }

    pub fn note(&mut self, time: Time, note: u8, vel: f32, dur: f32) {
        let end_time = match time {
            Time::Seconds(s) => Time::Seconds(s + dur),
            Time::Beats(b) => Time::Beats(b + dur),
        };
        self.desc
            .events
            .push((time, EventKind::NoteOn { note, vel }));
        self.desc
            .events
            .push((end_time, EventKind::NoteOff { note }));
    }

    pub fn chord(&mut self, time: Time, notes: &[u8], vel: f32, dur: f32) {
        for &n in notes {
            self.note(time, n, vel, dur);
        }
    }

    pub fn pattern(&mut self, start: Time, pattern: &Pattern) {
        for pn in pattern.notes() {
            let time = match start {
                Time::Seconds(s) => Time::Seconds(s + pn.beat),
                Time::Beats(b) => Time::Beats(b + pn.beat),
            };
            self.note(time, pn.note, pn.velocity, pn.duration);
        }
    }

    // ── Sub-function delegation (for granular hot-reload) ──

    /// Delegate to a sub-function for code organization.
    pub fn sound(&mut self, f: fn(&mut SceneTrack)) {
        f(self);
    }

    pub fn play(&mut self, f: fn(&mut SceneTrack)) {
        f(self);
    }

    pub fn fx(&mut self, f: fn(&mut SceneTrack)) {
        f(self);
    }
}
