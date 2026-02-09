use std::any::TypeId;
use std::collections::{HashMap, HashSet};

use subsecond::{HotFn, HotFnPtr};

use crate::automation::{AutomationFn, IntoVal, PatternFn, Val};
use crate::clip::Clip;
use crate::effect_config::EffectConfig;
use crate::engine::EngineHandle;
use crate::envelope::RetriggerMode;
use crate::event::{EventKind, Param};
use crate::filter::FilterType;
use crate::oscillator::Oscillator;
use crate::patch::Patch;
use crate::pattern::Pattern;
use crate::score::{Score, Time};
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

// ── Scene (stateful, long-lived) ──

pub struct Scene {
    bpm: f32,
    sample_rate: f32,
    handle: EngineHandle,
    names: HashMap<TrackId, String>,
    ptrs: HashMap<TrackId, HotFnPtr>,
    seen: HashSet<TrackId>,
}

impl Scene {
    pub fn new(bpm: f32, sample_rate: f32, handle: EngineHandle) -> Self {
        Self {
            bpm,
            sample_rate,
            handle,
            names: HashMap::new(),
            ptrs: HashMap::new(),
            seen: HashSet::new(),
        }
    }

    pub fn tempo(&mut self, bpm: f32) {
        if self.bpm != bpm {
            self.bpm = bpm;
            self.handle.set_tempo(crate::score::Tempo::new(bpm));
        }
    }

    /// Register a track. The function IS the identity.
    /// On hot-reload (ptr changed), tears down and rebuilds the track entirely.
    pub fn track<F: Fn(&mut SceneTrack) + 'static>(&mut self, f: F) {
        let id = TrackId::of::<F>();
        self.seen.insert(id);

        // Check if this track's function pointer changed
        let mut hot = HotFn::current(f);
        let ptr = hot.ptr_address();

        if let Some(old_ptr) = self.ptrs.get(&id) {
            if *old_ptr == ptr {
                return;
            }
        }

        // Function changed (or new track) — evaluate and rebuild
        self.ptrs.insert(id, ptr);

        let name = short_name::<F>();
        let mut builder = SceneTrack::new();
        hot.call((&mut builder,));

        // Tear down old track if it exists
        if self.names.contains_key(&id) {
            self.handle.remove_track(&name);
        }

        builder.send(&self.handle, &name, self.bpm, self.sample_rate);
        self.names.insert(id, name);
    }

    /// Call after all track() calls in a frame to detect removed tracks.
    pub fn finish_frame(&mut self) {
        let removed: Vec<TrackId> = self
            .names
            .keys()
            .filter(|id| !self.seen.contains(id))
            .copied()
            .collect();

        for id in removed {
            if let Some(name) = self.names.remove(&id) {
                self.handle.remove_track(&name);
            }
            self.ptrs.remove(&id);
        }

        self.seen.clear();
    }
}

// ── Track builder (user-facing, re-exported as `Track` in prelude) ──

pub struct SceneTrack {
    patch: Patch,
    polyphony: usize,
    effects: Vec<EffectConfig>,
    events: Vec<(Time, EventKind)>,
    loop_beats: Option<f32>,
    pattern_fn: Option<PatternFn>,
    automations: Vec<(Param, AutomationFn)>,
    oscs_set: bool,
}

impl SceneTrack {
    fn new() -> Self {
        Self {
            patch: Patch::new(),
            polyphony: 8,
            effects: Vec::new(),
            events: Vec::new(),
            loop_beats: None,
            pattern_fn: None,
            automations: Vec::new(),
            oscs_set: false,
        }
    }

    /// Send all track state to the audio thread via commands.
    fn send(self, handle: &EngineHandle, name: &str, bpm: f32, sr: f32) {
        let SceneTrack {
            patch,
            polyphony,
            effects,
            events,
            loop_beats,
            pattern_fn,
            automations,
            oscs_set: _,
        } = self;

        handle.add_track_with_polyphony(name, patch, polyphony);

        for effect in &effects {
            handle.add_effect_boxed(name, effect.build(bpm, sr));
        }

        // Looping pattern via every()
        if let Some(pf) = pattern_fn {
            let clip = Clip::looped("live", loop_beats.unwrap());
            handle.launch_with_pattern(name, clip, pf);
        }
        // One-shot notes (no every())
        else if !events.is_empty() {
            let mut clip = Clip::new("live");
            clip.score = Score::from_events(events);
            handle.launch(name, clip);
        }

        if !automations.is_empty() {
            handle.set_automations(name, automations);
        }
    }

    // ── Oscillator config ──

    /// Add an oscillator. First call clears the default oscillators.
    pub fn osc(&mut self, waveform: Waveform, level: f32) -> &mut Oscillator {
        if !self.oscs_set {
            self.patch.oscillators.clear();
            self.oscs_set = true;
        }
        self.patch
            .oscillators
            .push(Oscillator::new(waveform).level(level));
        self.patch.oscillators.last_mut().unwrap()
    }

    // ── Patch parameters (accept static f32 or |beat| -> f32 closures) ──

    pub fn gain(&mut self, v: impl IntoVal) {
        match v.into_val() {
            Val::Fixed(f) => self.patch.master_gain = f,
            Val::Fn(mut f) => {
                self.patch.master_gain = f(0.0);
                self.automations.push((Param::MasterGain, f));
            }
        }
    }

    pub fn attack(&mut self, v: impl IntoVal) {
        match v.into_val() {
            Val::Fixed(f) => self.patch.attack = f,
            Val::Fn(mut f) => {
                self.patch.attack = f(0.0);
                self.automations.push((Param::Attack, f));
            }
        }
    }

    pub fn decay(&mut self, v: impl IntoVal) {
        match v.into_val() {
            Val::Fixed(f) => self.patch.decay = f,
            Val::Fn(mut f) => {
                self.patch.decay = f(0.0);
                self.automations.push((Param::Decay, f));
            }
        }
    }

    pub fn sustain(&mut self, v: impl IntoVal) {
        match v.into_val() {
            Val::Fixed(f) => self.patch.sustain = f,
            Val::Fn(mut f) => {
                self.patch.sustain = f(0.0);
                self.automations.push((Param::Sustain, f));
            }
        }
    }

    pub fn release(&mut self, v: impl IntoVal) {
        match v.into_val() {
            Val::Fixed(f) => self.patch.release = f,
            Val::Fn(mut f) => {
                self.patch.release = f(0.0);
                self.automations.push((Param::Release, f));
            }
        }
    }

    pub fn cutoff(&mut self, v: impl IntoVal) {
        match v.into_val() {
            Val::Fixed(f) => self.patch.cutoff = f,
            Val::Fn(mut f) => {
                self.patch.cutoff = f(0.0);
                self.automations.push((Param::Cutoff, f));
            }
        }
    }

    pub fn resonance(&mut self, v: impl IntoVal) {
        match v.into_val() {
            Val::Fixed(f) => self.patch.resonance = f,
            Val::Fn(mut f) => {
                self.patch.resonance = f(0.0);
                self.automations.push((Param::Resonance, f));
            }
        }
    }

    pub fn lfo_rate(&mut self, v: impl IntoVal) {
        match v.into_val() {
            Val::Fixed(f) => self.patch.lfo_rate = f,
            Val::Fn(mut f) => {
                self.patch.lfo_rate = f(0.0);
                self.automations.push((Param::LfoRate, f));
            }
        }
    }

    pub fn lfo_depth(&mut self, v: impl IntoVal) {
        match v.into_val() {
            Val::Fixed(f) => self.patch.lfo_depth = f,
            Val::Fn(mut f) => {
                self.patch.lfo_depth = f(0.0);
                self.automations.push((Param::LfoDepth, f));
            }
        }
    }

    // ── Non-automatable params (discrete values) ──

    pub fn filter_type(&mut self, v: FilterType) {
        self.patch.filter_type = v;
    }

    pub fn retrigger(&mut self, v: RetriggerMode) {
        self.patch.retrigger = v;
    }

    pub fn polyphony(&mut self, n: usize) {
        self.polyphony = n;
    }

    // ── Effects ──

    pub fn delay(&mut self, f: impl FnOnce(&mut crate::effect_config::DelayBuilder)) {
        let mut b = crate::effect_config::DelayBuilder::new();
        f(&mut b);
        self.effects.push(EffectConfig::Delay(b.into_config()));
    }

    pub fn distortion(&mut self, f: impl FnOnce(&mut crate::effect_config::DistortionBuilder)) {
        let mut b = crate::effect_config::DistortionBuilder::new();
        f(&mut b);
        self.effects
            .push(EffectConfig::Distortion(b.into_config()));
    }

    pub fn chorus(&mut self, f: impl FnOnce(&mut crate::effect_config::ChorusBuilder)) {
        let mut b = crate::effect_config::ChorusBuilder::new();
        f(&mut b);
        self.effects.push(EffectConfig::Chorus(b.into_config()));
    }

    // ── Looping pattern ──

    /// Define a looping pattern. The closure re-runs at every loop boundary,
    /// so randomness naturally produces different results each loop.
    pub fn every(&mut self, beats: f32, f: impl FnMut(&mut crate::automation::Phrase) + Send + 'static) {
        self.loop_beats = Some(beats);
        self.pattern_fn = Some(Box::new(f));
    }

    // ── One-shot notes (no looping) ──

    pub fn note(&mut self, time: Time, note: u8, vel: f32, dur: f32) {
        let end_time = match time {
            Time::Seconds(s) => Time::Seconds(s + dur),
            Time::Beats(b) => Time::Beats(b + dur),
        };
        self.events
            .push((time, EventKind::NoteOn { note, vel }));
        self.events
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
