use std::any::TypeId;
use std::collections::{HashMap, HashSet};

use subsecond::{HotFn, HotFnPtr};
use tracing::{debug, trace};

use crate::automation::{AutoCmd, Automation, Clock, IntoVal, OscParam, PatternFn, Phrase, Val};
use crate::effect_config::EffectConfig;
use crate::engine::EngineHandle;
use crate::envelope::RetriggerMode;
use crate::event::SynthParam;
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

/// Comparable snapshot of a track's structural config (everything except closures).
#[derive(Clone, PartialEq)]
struct TrackSnapshot {
    patch: Patch,
    effects: Vec<EffectConfig>,
    loop_beats: Option<f32>,
}

pub struct Scene {
    bpm: f32,
    sample_rate: f32,
    handle: EngineHandle,
    names: HashMap<TrackId, String>,
    ptrs: HashMap<TrackId, HotFnPtr>,
    seen: HashSet<TrackId>,
    /// Previous structural state per track, for diffing on hot-reload.
    snapshots: HashMap<TrackId, TrackSnapshot>,
    /// MIDI input target (set by `midi()`, resolved in `finish_frame()`)
    midi_target: Option<TrackId>,
    /// Last MIDI target name sent to the engine (deduplication)
    midi_sent: Option<String>,
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
            snapshots: HashMap::new(),
            midi_target: None,
            midi_sent: None,
        }
    }

    pub fn tempo(&mut self, bpm: f32) {
        if self.bpm != bpm {
            self.bpm = bpm;
            self.handle.set_tempo(crate::score::Tempo::new(bpm));
        }
    }

    /// Register a track. The function IS the identity.
    /// On hot-reload (ptr changed), queues a boundary-aligned update for existing
    /// tracks. New tracks are created immediately.
    pub fn track<F: Fn(&mut SceneTrack) + 'static>(&mut self, f: F) {
        let id = TrackId::of::<F>();
        self.seen.insert(id);

        // Check if this track's function pointer changed
        let mut hot = HotFn::current(f);
        let ptr = hot.ptr_address();

        if let Some(old_ptr) = self.ptrs.get(&id)
            && *old_ptr == ptr {
                return;
            }

        // Function changed (or new track) — evaluate the builder
        self.ptrs.insert(id, ptr);

        let name = short_name::<F>();
        let mut builder = SceneTrack::new();
        hot.call((&mut builder,));

        let new_snap = TrackSnapshot {
            patch: builder.patch().clone(),
            effects: builder.effect_configs().to_vec(),
            loop_beats: builder.loop_beats(),
        };

        if self.names.contains_key(&id) {
            // Existing track — compare structural output to decide what to do
            let prev = self.snapshots.get(&id);
            if prev == Some(&new_snap) {
                // Nothing structural changed — closures auto-update via subsecond
                trace!(track = %name, "skip (unchanged)");
                self.snapshots.insert(id, new_snap);
                return;
            }
            let fx_changed = prev.map(|p| p.effects != new_snap.effects).unwrap_or(true);
            debug!(track = %name, fx_changed, "update at boundary");
            self.snapshots.insert(id, new_snap);
            builder.send_update(&self.handle, &name, self.bpm, self.sample_rate, fx_changed);
        } else {
            // New track — create and launch immediately
            debug!(track = %name, "add new track");
            self.snapshots.insert(id, new_snap);
            builder.send(&self.handle, &name, self.bpm, self.sample_rate);
            self.names.insert(id, name);
        }
    }

    /// Route MIDI input to the given track's synth.
    pub fn midi<F: Fn(&mut SceneTrack) + 'static>(&mut self, _f: F) {
        self.midi_target = Some(TrackId::of::<F>());
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
            self.snapshots.remove(&id);
        }

        // Resolve MIDI target and send if changed
        if let Some(id) = self.midi_target {
            if let Some(name) = self.names.get(&id) {
                if self.midi_sent.as_ref() != Some(name) {
                    self.handle.set_midi_track(name);
                    self.midi_sent = Some(name.clone());
                }
            }
        }

        self.seen.clear();
    }
}

// ── Track builder (user-facing, re-exported as `Track` in prelude) ──

pub struct SceneTrack {
    patch: Patch,
    polyphony: usize,
    effects: Vec<EffectConfig>,
    fx_enabled: Vec<bool>,
    phrase: Phrase,
    loop_beats: Option<f32>,
    pattern_fn: Option<PatternFn>,
    automations: Vec<Automation>,
    oscs_set: bool,
}

impl SceneTrack {
    fn new() -> Self {
        Self {
            patch: Patch::new(),
            polyphony: 8,
            effects: Vec::new(),
            fx_enabled: Vec::new(),
            phrase: Phrase::new(),
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
            fx_enabled,
            phrase,
            loop_beats,
            pattern_fn,
            automations,
            oscs_set: _,
        } = self;

        handle.add_track_with_polyphony(name, patch, polyphony);

        for effect in &effects {
            handle.add_effect_boxed(name, effect.build(bpm, sr));
        }

        if !fx_enabled.is_empty() {
            handle.set_fx_enabled(name, fx_enabled);
        }

        // Looping pattern via every()
        if let Some(pf) = pattern_fn {
            handle.launch_with_pattern(name, loop_beats.unwrap(), pf);
        }
        // One-shot notes (no every())
        else {
            let events = phrase.into_events();
            if !events.is_empty() {
                handle.launch(name, Score::from_events(events));
            }
        }

        if !automations.is_empty() {
            handle.set_automations(name, automations);
        }
    }

    fn patch(&self) -> &Patch {
        &self.patch
    }

    fn effect_configs(&self) -> &[EffectConfig] {
        &self.effects
    }

    fn loop_beats(&self) -> Option<f32> {
        self.loop_beats
    }

    /// Send a boundary-aligned update for an existing track (no teardown).
    /// When `fx_changed` is false, effects are omitted to preserve delay buffers etc.
    fn send_update(self, handle: &EngineHandle, name: &str, bpm: f32, sr: f32, fx_changed: bool) {
        let SceneTrack {
            patch,
            polyphony: _,
            effects,
            fx_enabled,
            phrase: _,
            loop_beats,
            pattern_fn,
            automations,
            oscs_set: _,
        } = self;

        let built_effects = if fx_changed {
            Some((
                effects.iter().map(|e| e.build(bpm, sr)).collect(),
                fx_enabled,
            ))
        } else {
            None
        };

        handle.update_track(name, patch, built_effects, automations, pattern_fn, loop_beats);
    }

    // ── Oscillator config ──

    /// Add an oscillator. First call clears the default oscillators.
    /// Returns an `OscBuilder` for setting detune, level, waveform automations.
    pub fn osc(&mut self, waveform: Waveform, level: f32) -> OscBuilder<'_> {
        if !self.oscs_set {
            self.patch.oscillators.clear();
            self.oscs_set = true;
        }
        self.patch
            .oscillators
            .push(Oscillator::new(waveform).level(level));
        let osc_index = self.patch.oscillators.len() - 1;
        OscBuilder {
            track: self,
            osc_index,
        }
    }

    // ── Patch parameters (accept static or closure) ──

    pub fn gain(&mut self, v: impl IntoVal<f32>) {
        match v.into_val() {
            Val::Fixed(f) => self.patch.master_gain = f,
            Val::Fn(mut f) => {
                self.patch.master_gain = f(Clock::ZERO);
                self.automations.push(Box::new(move |c| AutoCmd::Synth(SynthParam::MasterGain, f(c))));
            }
        }
    }

    pub fn attack(&mut self, v: impl IntoVal<f32>) {
        match v.into_val() {
            Val::Fixed(f) => self.patch.attack = f,
            Val::Fn(mut f) => {
                self.patch.attack = f(Clock::ZERO);
                self.automations.push(Box::new(move |c| AutoCmd::Synth(SynthParam::Attack, f(c))));
            }
        }
    }

    pub fn decay(&mut self, v: impl IntoVal<f32>) {
        match v.into_val() {
            Val::Fixed(f) => self.patch.decay = f,
            Val::Fn(mut f) => {
                self.patch.decay = f(Clock::ZERO);
                self.automations.push(Box::new(move |c| AutoCmd::Synth(SynthParam::Decay, f(c))));
            }
        }
    }

    pub fn sustain(&mut self, v: impl IntoVal<f32>) {
        match v.into_val() {
            Val::Fixed(f) => self.patch.sustain = f,
            Val::Fn(mut f) => {
                self.patch.sustain = f(Clock::ZERO);
                self.automations.push(Box::new(move |c| AutoCmd::Synth(SynthParam::Sustain, f(c))));
            }
        }
    }

    pub fn release(&mut self, v: impl IntoVal<f32>) {
        match v.into_val() {
            Val::Fixed(f) => self.patch.release = f,
            Val::Fn(mut f) => {
                self.patch.release = f(Clock::ZERO);
                self.automations.push(Box::new(move |c| AutoCmd::Synth(SynthParam::Release, f(c))));
            }
        }
    }

    pub fn cutoff(&mut self, v: impl IntoVal<f32>) {
        match v.into_val() {
            Val::Fixed(f) => self.patch.cutoff = f,
            Val::Fn(mut f) => {
                self.patch.cutoff = f(Clock::ZERO);
                self.automations.push(Box::new(move |c| AutoCmd::Synth(SynthParam::Cutoff, f(c))));
            }
        }
    }

    pub fn resonance(&mut self, v: impl IntoVal<f32>) {
        match v.into_val() {
            Val::Fixed(f) => self.patch.resonance = f,
            Val::Fn(mut f) => {
                self.patch.resonance = f(Clock::ZERO);
                self.automations.push(Box::new(move |c| AutoCmd::Synth(SynthParam::Resonance, f(c))));
            }
        }
    }

    pub fn lfo_rate(&mut self, v: impl IntoVal<f32>) {
        match v.into_val() {
            Val::Fixed(f) => self.patch.lfo_rate = f,
            Val::Fn(mut f) => {
                self.patch.lfo_rate = f(Clock::ZERO);
                self.automations.push(Box::new(move |c| AutoCmd::Synth(SynthParam::LfoRate, f(c))));
            }
        }
    }

    pub fn lfo_depth(&mut self, v: impl IntoVal<f32>) {
        match v.into_val() {
            Val::Fixed(f) => self.patch.lfo_depth = f,
            Val::Fn(mut f) => {
                self.patch.lfo_depth = f(Clock::ZERO);
                self.automations.push(Box::new(move |c| AutoCmd::Synth(SynthParam::LfoDepth, f(c))));
            }
        }
    }

    // ── Discrete params (now automatable) ──

    pub fn filter_type(&mut self, v: impl IntoVal<FilterType>) {
        match v.into_val() {
            Val::Fixed(ft) => self.patch.filter_type = ft,
            Val::Fn(mut f) => {
                self.patch.filter_type = f(Clock::ZERO);
                self.automations.push(Box::new(move |c| AutoCmd::FilterType(f(c))));
            }
        }
    }

    pub fn retrigger(&mut self, v: impl IntoVal<RetriggerMode>) {
        match v.into_val() {
            Val::Fixed(m) => self.patch.retrigger = m,
            Val::Fn(mut f) => {
                self.patch.retrigger = f(Clock::ZERO);
                self.automations.push(Box::new(move |c| AutoCmd::Retrigger(f(c))));
            }
        }
    }

    pub fn polyphony(&mut self, n: usize) {
        self.polyphony = n;
    }

    // ── Effects ──

    pub fn delay(&mut self, f: impl FnOnce(&mut crate::effect_config::DelayBuilder)) {
        let fx_index = self.effects.len();
        let mut b = crate::effect_config::DelayBuilder::new();
        f(&mut b);
        let enabled = b.initial_enabled();
        let param_autos = b.take_automations();
        let enabled_auto = b.take_enabled_auto();
        self.effects.push(EffectConfig::Delay(b.into_config()));
        self.fx_enabled.push(enabled);
        self.collect_fx_automations(fx_index, param_autos, enabled_auto);
    }

    pub fn distortion(&mut self, f: impl FnOnce(&mut crate::effect_config::DistortionBuilder)) {
        let fx_index = self.effects.len();
        let mut b = crate::effect_config::DistortionBuilder::new();
        f(&mut b);
        let enabled = b.initial_enabled();
        let param_autos = b.take_automations();
        let enabled_auto = b.take_enabled_auto();
        self.effects
            .push(EffectConfig::Distortion(b.into_config()));
        self.fx_enabled.push(enabled);
        self.collect_fx_automations(fx_index, param_autos, enabled_auto);
    }

    pub fn chorus(&mut self, f: impl FnOnce(&mut crate::effect_config::ChorusBuilder)) {
        let fx_index = self.effects.len();
        let mut b = crate::effect_config::ChorusBuilder::new();
        f(&mut b);
        let enabled = b.initial_enabled();
        let param_autos = b.take_automations();
        let enabled_auto = b.take_enabled_auto();
        self.effects.push(EffectConfig::Chorus(b.into_config()));
        self.fx_enabled.push(enabled);
        self.collect_fx_automations(fx_index, param_autos, enabled_auto);
    }

    fn collect_fx_automations(
        &mut self,
        fx_index: usize,
        param_autos: Vec<(u8, Box<dyn FnMut(Clock) -> f32 + Send>)>,
        enabled_auto: Option<Box<dyn FnMut(Clock) -> bool + Send>>,
    ) {
        for (slot, mut closure) in param_autos {
            self.automations.push(Box::new(move |c| {
                AutoCmd::FxParam { fx_index, slot, value: closure(c) }
            }));
        }
        if let Some(mut ef) = enabled_auto {
            self.automations.push(Box::new(move |c| {
                AutoCmd::FxEnabled { fx_index, enabled: ef(c) }
            }));
        }
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
        self.phrase.note(time, note, vel, dur);
    }

    pub fn chord(&mut self, time: Time, notes: &[u8], vel: f32, dur: f32) {
        self.phrase.chord(time, notes, vel, dur);
    }

    pub fn pattern(&mut self, start: Time, pattern: &Pattern) {
        self.phrase.pattern(start, pattern);
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

// ── OscBuilder: returned by SceneTrack::osc() ──

pub struct OscBuilder<'a> {
    track: &'a mut SceneTrack,
    osc_index: usize,
}

impl<'a> OscBuilder<'a> {
    pub fn detune(self, v: impl IntoVal<f32>) -> Self {
        match v.into_val() {
            Val::Fixed(f) => {
                self.track.patch.oscillators[self.osc_index].detune_semitones = f;
            }
            Val::Fn(mut f) => {
                self.track.patch.oscillators[self.osc_index].detune_semitones = f(Clock::ZERO);
                let idx = self.osc_index;
                self.track.automations.push(Box::new(move |c| {
                    AutoCmd::OscParam { osc_index: idx, param: OscParam::Detune, value: f(c) }
                }));
            }
        }
        self
    }

    pub fn level(self, v: impl IntoVal<f32>) -> Self {
        match v.into_val() {
            Val::Fixed(f) => {
                self.track.patch.oscillators[self.osc_index].level = f;
            }
            Val::Fn(mut f) => {
                self.track.patch.oscillators[self.osc_index].level = f(Clock::ZERO);
                let idx = self.osc_index;
                self.track.automations.push(Box::new(move |c| {
                    AutoCmd::OscParam { osc_index: idx, param: OscParam::Level, value: f(c) }
                }));
            }
        }
        self
    }

    pub fn waveform(self, v: impl IntoVal<Waveform>) -> Self {
        match v.into_val() {
            Val::Fixed(w) => {
                self.track.patch.oscillators[self.osc_index].waveform = w;
            }
            Val::Fn(mut f) => {
                self.track.patch.oscillators[self.osc_index].waveform = f(Clock::ZERO);
                let idx = self.osc_index;
                self.track.automations.push(Box::new(move |c| {
                    AutoCmd::OscWaveform { osc_index: idx, waveform: f(c) }
                }));
            }
        }
        self
    }

    pub fn phase_offset(self, offset: f32) -> Self {
        self.track.patch.oscillators[self.osc_index].phase_offset = offset;
        self
    }
}
