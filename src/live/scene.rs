use std::any::TypeId;
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::path::PathBuf;

use tracing::debug;

use crate::dsp::filter::FilterType;
use crate::dsp::oscillator::Oscillator;
use crate::dsp::envelope::RetriggerMode;
use crate::dsp::waveform::Waveform;
use crate::engine::command::{
    BuiltSource, Command, EngineHandle, GroupBuild, Playback, TrackBuild,
};
use crate::engine::instrument::SampleSource;
use crate::model::fx::{FxKind, FxSpec};
use crate::model::param::{IntoVal, ParamId, Val};
use crate::model::patch::PatchSpec;
use crate::model::track::{SourceSpec, Swing, TrackSpec};
use crate::music::{Clock, NoteSpec, Phrase};
use crate::sample::SampleCache;

// ── Identity ──

/// A track's identity: the `TypeId` of its builder function. The function *is*
/// the track — no registration, no string keys in user code.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TrackId(TypeId);

impl TrackId {
    pub fn of<F: 'static>() -> Self {
        Self(TypeId::of::<F>())
    }
}

/// Full module path used as the engine key (so `a::bass` and `b::bass` don't
/// collide); the last segment is the display name.
fn track_key<F: 'static>() -> String {
    std::any::type_name::<F>().to_string()
}

fn short_name(key: &str) -> &str {
    key.rsplit("::").find(|s| !s.starts_with('{')).unwrap_or("track")
}

// ── Diffable snapshot ──

#[derive(Clone, PartialEq)]
struct TrackDiff {
    source: SourceSpec,
    patch: PatchSpec,
    gain: f32,
    pan: f32,
    mute: bool,
    fx: Vec<FxSpec>,
    polyphony: usize,
}

impl TrackDiff {
    fn of(spec: &TrackSpec) -> Self {
        Self {
            source: spec.source.clone(),
            patch: spec.patch.clone(),
            gain: spec.gain,
            pan: spec.pan,
            mute: spec.mute,
            fx: spec.fx.clone(),
            polyphony: spec.polyphony,
        }
    }
}

#[derive(Clone, PartialEq)]
struct GroupCfg {
    members: Vec<TrackId>,
    gain: f32,
    fx: Vec<FxSpec>,
}

// ── Scene ──

pub struct Scene {
    handle: EngineHandle,
    bpm: f32,
    sample_rate: f32,
    names: HashMap<TrackId, String>,
    #[cfg(feature = "hot-reload")]
    ptrs: HashMap<TrackId, subsecond::HotFnPtr>,
    #[cfg(not(feature = "hot-reload"))]
    evaluated: HashSet<TrackId>,
    prev: HashMap<TrackId, TrackDiff>,
    seen: HashSet<TrackId>,
    sample_cache: SampleCache,
    midi_target: Option<TrackId>,
    midi_sent: Option<Option<String>>,
    groups_frame: Vec<GroupCfg>,
    prev_groups: Vec<GroupCfg>,
}

impl Scene {
    pub fn new(bpm: f32, sample_rate: f32, handle: EngineHandle) -> Self {
        Self {
            handle,
            bpm,
            sample_rate,
            names: HashMap::new(),
            #[cfg(feature = "hot-reload")]
            ptrs: HashMap::new(),
            #[cfg(not(feature = "hot-reload"))]
            evaluated: HashSet::new(),
            prev: HashMap::new(),
            seen: HashSet::new(),
            sample_cache: SampleCache::new(),
            midi_target: None,
            midi_sent: None,
            groups_frame: Vec::new(),
            prev_groups: Vec::new(),
        }
    }

    pub fn tempo(&mut self, bpm: f32) {
        if self.bpm != bpm {
            self.bpm = bpm;
            self.handle.send(Command::SetTempo(bpm));
        }
    }

    /// Register a track. The function IS the identity. On hot-reload the builder
    /// re-runs only when the function pointer changes; the resulting spec is
    /// diffed against the previous frame to decide what to send.
    pub fn track<F: Fn(&mut SceneTrack) + 'static>(&mut self, f: F) {
        let id = TrackId::of::<F>();
        self.seen.insert(id);
        let key = track_key::<F>();

        #[cfg(feature = "hot-reload")]
        {
            let mut hot = subsecond::HotFn::current(f);
            let ptr = hot.ptr_address();
            if self.ptrs.get(&id) == Some(&ptr) && self.names.contains_key(&id) {
                return;
            }
            self.ptrs.insert(id, ptr);
            let mut builder = SceneTrack::new();
            hot.call((&mut builder,));
            self.apply_spec(id, key, builder.into_spec());
        }

        #[cfg(not(feature = "hot-reload"))]
        {
            if !self.evaluated.insert(id) {
                return;
            }
            let mut builder = SceneTrack::new();
            f(&mut builder);
            self.apply_spec(id, key, builder.into_spec());
        }
    }

    /// Route MIDI input to the given track.
    pub fn midi<F: Fn(&mut SceneTrack) + 'static>(&mut self, _f: F) {
        self.midi_target = Some(TrackId::of::<F>());
    }

    /// Define a group bus: sum several tracks through shared gain and effects.
    pub fn group(&mut self, _name: &str, build: impl FnOnce(&mut GroupBuilder)) {
        let mut g = GroupBuilder::new();
        build(&mut g);
        self.groups_frame.push(GroupCfg {
            members: g.members,
            gain: g.gain,
            fx: g.fx,
        });
    }

    /// Call after all builder calls in a frame: detect removed tracks, reconcile
    /// groups, and resolve the MIDI target.
    pub fn finish_frame(&mut self) {
        let removed: Vec<TrackId> = self
            .names
            .keys()
            .filter(|id| !self.seen.contains(id))
            .copied()
            .collect();
        for id in removed {
            if let Some(name) = self.names.remove(&id) {
                self.handle.send(Command::RemoveTrack(name));
            }
            self.prev.remove(&id);
            #[cfg(feature = "hot-reload")]
            self.ptrs.remove(&id);
        }

        // Groups: send the whole set only when it changed.
        if self.groups_frame != self.prev_groups {
            let builds = self
                .groups_frame
                .iter()
                .filter_map(|g| self.build_group(g))
                .collect();
            self.handle.send(Command::SetGroups(builds));
            self.prev_groups = std::mem::take(&mut self.groups_frame);
        } else {
            self.groups_frame.clear();
        }

        // MIDI target.
        let target = self
            .midi_target
            .and_then(|id| self.names.get(&id).cloned());
        if self.midi_sent.as_ref() != Some(&target) {
            self.handle.send(Command::SetMidiTrack(target.clone()));
            self.midi_sent = Some(target);
        }

        self.seen.clear();
    }

    // ── Diff → commands ──

    fn apply_spec(&mut self, id: TrackId, key: String, spec: TrackSpec) {
        let new = TrackDiff::of(&spec);
        let name = short_name(&key);

        match self.prev.get(&id).cloned() {
            None => {
                debug!(track = name, "add new track");
                let build = self.make_build(&key, spec);
                self.handle.send(Command::AddTrack {
                    name: key.clone(),
                    build: Box::new(build),
                });
                self.names.insert(id, key);
                self.prev.insert(id, new);
            }
            Some(prev) => {
                if new.source != prev.source || new.polyphony != prev.polyphony {
                    debug!(track = name, "source changed — rebuild");
                    self.handle.send(Command::RemoveTrack(key.clone()));
                    let build = self.make_build(&key, spec);
                    self.handle.send(Command::AddTrack {
                        name: key.clone(),
                        build: Box::new(build),
                    });
                } else {
                    self.send_updates(&key, &prev, &new, spec);
                }
                self.prev.insert(id, new);
            }
        }
    }

    fn send_updates(&mut self, key: &str, prev: &TrackDiff, new: &TrackDiff, spec: TrackSpec) {
        let TrackSpec {
            patch,
            gain,
            pan,
            mute,
            fx,
            loop_len,
            pattern,
            one_shot,
            automations,
            swing,
            ..
        } = spec;

        if new.patch != prev.patch {
            self.handle.send(Command::SetPatch {
                track: key.to_string(),
                patch,
            });
        }
        if new.gain != prev.gain || new.pan != prev.pan || new.mute != prev.mute {
            self.handle.send(Command::SetMixer {
                track: key.to_string(),
                gain,
                pan,
                mute,
            });
        }
        if new.fx != prev.fx {
            if fx_only_enabled_changed(&prev.fx, &new.fx) {
                self.handle.send(Command::SetFxEnabled {
                    track: key.to_string(),
                    enabled: new.fx.iter().map(|f| f.enabled).collect(),
                });
            } else {
                let (boxes, enabled) = self.build_fx(&fx);
                self.handle.send(Command::SetFx {
                    track: key.to_string(),
                    fx: boxes,
                    enabled,
                });
            }
        }

        // Automations can't be value-compared, so resend whenever we re-ran.
        self.handle.send(Command::SetAutomations {
            track: key.to_string(),
            automations,
        });

        // Timing changes queue to the loop boundary.
        self.send_timing(key, pattern, loop_len, one_shot, swing);
    }

    fn send_timing(
        &mut self,
        key: &str,
        pattern: Option<crate::model::param::PatternFn>,
        loop_len: Option<f32>,
        one_shot: Vec<NoteSpec>,
        swing: Option<Swing>,
    ) {
        match (pattern, loop_len) {
            (Some(func), Some(len)) => self.handle.send(Command::QueuePattern {
                track: key.to_string(),
                func,
                loop_len: len as f64,
                swing,
            }),
            _ if !one_shot.is_empty() => self.handle.send(Command::QueueOneShot {
                track: key.to_string(),
                notes: one_shot,
                swing,
            }),
            _ => {}
        }
    }

    // ── Build helpers ──

    fn make_build(&mut self, key: &str, spec: TrackSpec) -> TrackBuild {
        let source = self.resolve_source(spec.source);
        let (fx, fx_enabled) = self.build_fx(&spec.fx);
        let swing = spec.swing;
        let playback = match (spec.pattern, spec.loop_len) {
            (Some(func), Some(len)) => Playback::Pattern {
                func,
                loop_len: len as f64,
                swing,
            },
            _ if !spec.one_shot.is_empty() => Playback::OneShot {
                notes: spec.one_shot,
                swing,
            },
            _ => Playback::Silent,
        };
        TrackBuild {
            source,
            patch: spec.patch,
            polyphony: spec.polyphony,
            gain: spec.gain,
            pan: spec.pan,
            mute: spec.mute,
            fx,
            fx_enabled,
            automations: spec.automations,
            playback,
            seed: hash_key(key),
        }
    }

    fn resolve_source(&mut self, source: SourceSpec) -> BuiltSource {
        match source {
            SourceSpec::Synth => BuiltSource::Synth,
            SourceSpec::Sample { path, root } => match self.sample_cache.get(&path) {
                Some(data) => BuiltSource::Sample(SampleSource::Pitched { data, root }),
                None => BuiltSource::Synth,
            },
            SourceSpec::Kit { slots } => {
                let mut map = HashMap::new();
                for (note, path) in slots {
                    if let Some(data) = self.sample_cache.get(&path) {
                        map.insert(note, data);
                    }
                }
                BuiltSource::Sample(SampleSource::Kit { map })
            }
        }
    }

    fn build_fx(&self, fx: &[FxSpec]) -> (Vec<Box<dyn crate::dsp::effects::Effect>>, Vec<bool>) {
        let boxes = fx.iter().map(|f| f.build(self.bpm, self.sample_rate)).collect();
        let enabled = fx.iter().map(|f| f.enabled).collect();
        (boxes, enabled)
    }

    fn build_group(&self, g: &GroupCfg) -> Option<GroupBuild> {
        let members: Vec<String> = g
            .members
            .iter()
            .filter_map(|id| self.names.get(id).cloned())
            .collect();
        if members.is_empty() {
            return None;
        }
        let (fx, fx_enabled) = self.build_fx(&g.fx);
        Some(GroupBuild {
            members,
            gain: g.gain,
            fx,
            fx_enabled,
        })
    }
}

fn fx_only_enabled_changed(a: &[FxSpec], b: &[FxSpec]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.kind == y.kind)
}

/// FNV-1a hash of the track key — a stable per-track RNG seed so pattern
/// regeneration re-rolls per iteration yet renders bit-reproducibly.
fn hash_key(key: &str) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for b in key.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

// ── SceneTrack: the one builder surface ──

pub struct SceneTrack {
    spec: TrackSpec,
    oscs_set: bool,
    kit_next: u8,
}

impl SceneTrack {
    fn new() -> Self {
        Self {
            spec: TrackSpec::default(),
            oscs_set: false,
            kit_next: 24, // C1
        }
    }

    fn into_spec(self) -> TrackSpec {
        self.spec
    }

    /// Set an f32 param, recording an automation if given a closure.
    fn val(&mut self, v: impl IntoVal<f32>, id: ParamId, apply: impl FnOnce(&mut TrackSpec, f32)) {
        match v.into_val() {
            Val::Fixed(f) => apply(&mut self.spec, f),
            Val::Fn(mut f) => {
                let init = f(Clock::ZERO);
                apply(&mut self.spec, init);
                self.spec.automations.push((id, f));
            }
        }
    }

    // ── Oscillators ──

    pub fn osc(&mut self, waveform: Waveform, level: f32) -> OscBuilder<'_> {
        if !self.oscs_set {
            self.spec.patch.oscillators.clear();
            self.oscs_set = true;
        }
        self.spec
            .patch
            .oscillators
            .push(Oscillator::new(waveform).level(level));
        let idx = self.spec.patch.oscillators.len() - 1;
        OscBuilder { track: self, idx }
    }

    // ── Envelope / filter / LFO ──

    pub fn attack(&mut self, v: impl IntoVal<f32>) {
        self.val(v, ParamId::Attack, |s, f| s.patch.attack = f);
    }
    pub fn decay(&mut self, v: impl IntoVal<f32>) {
        self.val(v, ParamId::Decay, |s, f| s.patch.decay = f);
    }
    pub fn sustain(&mut self, v: impl IntoVal<f32>) {
        self.val(v, ParamId::Sustain, |s, f| s.patch.sustain = f);
    }
    pub fn release(&mut self, v: impl IntoVal<f32>) {
        self.val(v, ParamId::Release, |s, f| s.patch.release = f);
    }
    pub fn cutoff(&mut self, v: impl IntoVal<f32>) {
        self.val(v, ParamId::Cutoff, |s, f| s.patch.cutoff = f);
    }
    pub fn resonance(&mut self, v: impl IntoVal<f32>) {
        self.val(v, ParamId::Resonance, |s, f| s.patch.resonance = f);
    }
    pub fn lfo_rate(&mut self, v: impl IntoVal<f32>) {
        self.val(v, ParamId::LfoRate, |s, f| s.patch.lfo_rate = f);
    }
    pub fn lfo_depth(&mut self, v: impl IntoVal<f32>) {
        self.val(v, ParamId::LfoDepth, |s, f| s.patch.lfo_depth = f);
    }

    /// Convenience: set the full ADSR envelope in one call.
    pub fn adsr(
        &mut self,
        a: impl IntoVal<f32>,
        d: impl IntoVal<f32>,
        s: impl IntoVal<f32>,
        r: impl IntoVal<f32>,
    ) {
        self.attack(a);
        self.decay(d);
        self.sustain(s);
        self.release(r);
    }

    pub fn filter(&mut self, ft: FilterType) {
        self.spec.patch.filter_type = ft;
    }
    pub fn retrigger(&mut self, mode: RetriggerMode) {
        self.spec.patch.retrigger = mode;
    }
    pub fn polyphony(&mut self, n: usize) {
        self.spec.polyphony = n;
    }

    /// Swing: shift notes on odd multiples of `grid` late by `amount`. `0.5` is
    /// straight; `0.57` a gentle shuffle. See [`Swing`](crate::model::Swing).
    pub fn swing(&mut self, grid: f32, amount: f32) {
        self.spec.swing = Some(Swing { grid, amount });
    }

    // ── Mixer ──

    pub fn gain(&mut self, v: impl IntoVal<f32>) {
        self.val(v, ParamId::Gain, |s, f| s.gain = f);
    }
    pub fn pan(&mut self, v: impl IntoVal<f32>) {
        self.val(v, ParamId::Pan, |s, f| s.pan = f);
    }
    pub fn mute(&mut self, on: bool) {
        self.spec.mute = on;
    }

    // ── Sample sources ──

    pub fn sample(&mut self, path: &str) {
        self.spec.source = SourceSpec::Sample {
            path: PathBuf::from(path),
            root: 60,
        };
    }

    pub fn root(&mut self, note: u8) {
        if let SourceSpec::Sample { root, .. } = &mut self.spec.source {
            *root = note;
        } else {
            self.spec.source = SourceSpec::Sample {
                path: PathBuf::new(),
                root: note,
            };
        }
    }

    pub fn slot(&mut self, path: &str) -> u8 {
        let note = self.kit_next;
        self.kit_next = self.kit_next.saturating_add(1);
        self.push_slot(note, path);
        note
    }

    pub fn slot_at(&mut self, note: u8, path: &str) -> u8 {
        if note >= self.kit_next {
            self.kit_next = note.saturating_add(1);
        }
        self.push_slot(note, path);
        note
    }

    fn push_slot(&mut self, note: u8, path: &str) {
        let entry = (note, PathBuf::from(path));
        match &mut self.spec.source {
            SourceSpec::Kit { slots } => slots.push(entry),
            _ => self.spec.source = SourceSpec::Kit { slots: vec![entry] },
        }
    }

    // ── Effects (constructors live in fx_builder.rs; these are the shared
    // mutation helpers the builders call) ──

    /// Push a default fx of the given kind and return its index.
    pub(crate) fn push_fx_default(&mut self, kind: FxKind) -> usize {
        let idx = self.spec.fx.len();
        self.spec.fx.push(FxSpec { kind, enabled: true });
        idx
    }

    /// Set an fx parameter from a `Val`, recording an `Fx { index, slot }`
    /// automation when it is a closure/shape. `apply` writes the fixed value
    /// into the effect's config.
    pub(crate) fn fx_param(
        &mut self,
        index: usize,
        slot: u8,
        v: impl IntoVal<f32>,
        apply: impl FnOnce(&mut FxKind, f32),
    ) {
        match v.into_val() {
            Val::Fixed(f) => apply(&mut self.spec.fx[index].kind, f),
            Val::Fn(mut f) => {
                let init = f(Clock::ZERO);
                apply(&mut self.spec.fx[index].kind, init);
                self.spec.automations.push((ParamId::Fx { index, slot }, f));
            }
        }
    }

    pub(crate) fn fx_kind_mut(&mut self, index: usize) -> &mut FxKind {
        &mut self.spec.fx[index].kind
    }

    pub(crate) fn set_fx_enabled_flag(&mut self, index: usize, on: bool) {
        self.spec.fx[index].enabled = on;
    }

    // ── Playback ──

    pub fn every(&mut self, beats: f32, f: impl FnMut(&mut Phrase) + Send + 'static) {
        self.spec.loop_len = Some(beats);
        self.spec.pattern = Some(Box::new(f));
    }

    /// Play a single note (defaults: beat 0.0, velocity 0.8). Chain `.at(beat)`
    /// / `.vel(v)` on the returned guard.
    pub fn note(&mut self, note: u8, dur: f32) -> OneShotNote<'_> {
        let idx = self.spec.one_shot.len();
        self.spec.one_shot.push(NoteSpec { beat: 0.0, note, vel: 0.8, dur });
        OneShotNote { one_shot: &mut self.spec.one_shot, range: idx..idx + 1 }
    }

    /// Play a chord (defaults: beat 0.0, velocity 0.8).
    pub fn chord(&mut self, notes: &[u8], dur: f32) -> OneShotNote<'_> {
        let idx = self.spec.one_shot.len();
        for &n in notes {
            self.spec.one_shot.push(NoteSpec { beat: 0.0, note: n, vel: 0.8, dur });
        }
        let end = self.spec.one_shot.len();
        OneShotNote { one_shot: &mut self.spec.one_shot, range: idx..end }
    }
}

/// Guard over a one-shot note/chord: `.at(beat)` repositions, `.vel(v)` sets
/// velocity. Mirrors [`Phrase`](crate::music::Phrase)'s note guard.
pub struct OneShotNote<'a> {
    one_shot: &'a mut Vec<NoteSpec>,
    range: Range<usize>,
}

impl OneShotNote<'_> {
    pub fn at(self, beat: f32) -> Self {
        for n in &mut self.one_shot[self.range.clone()] {
            n.beat = beat;
        }
        self
    }

    pub fn vel(self, v: f32) -> Self {
        for n in &mut self.one_shot[self.range.clone()] {
            n.vel = v;
        }
        self
    }
}

// ── OscBuilder ──

pub struct OscBuilder<'a> {
    track: &'a mut SceneTrack,
    idx: usize,
}

impl OscBuilder<'_> {
    pub fn detune(self, v: impl IntoVal<f32>) -> Self {
        let idx = self.idx;
        self.track.val(v, ParamId::OscDetune(idx), move |s, f| {
            s.patch.oscillators[idx].detune_semitones = f;
        });
        self
    }

    pub fn level(self, v: impl IntoVal<f32>) -> Self {
        let idx = self.idx;
        self.track.val(v, ParamId::OscLevel(idx), move |s, f| {
            s.patch.oscillators[idx].level = f;
        });
        self
    }

    pub fn phase_offset(self, offset: f32) -> Self {
        self.track.spec.patch.oscillators[self.idx].phase_offset = offset;
        self
    }
}

// ── GroupBuilder ──

pub struct GroupBuilder {
    members: Vec<TrackId>,
    gain: f32,
    fx: Vec<FxSpec>,
}

impl GroupBuilder {
    fn new() -> Self {
        Self {
            members: Vec::new(),
            gain: 1.0,
            fx: Vec::new(),
        }
    }

    /// Add a track to this group by its builder function.
    pub fn track<F: Fn(&mut SceneTrack) + 'static>(&mut self, _f: F) {
        self.members.push(TrackId::of::<F>());
    }

    pub fn gain(&mut self, g: f32) {
        self.gain = g;
    }

    /// Push a default fx of the given kind and return its index. Group fx take
    /// plain `f32` setters — automation is unsupported on buses by construction.
    pub(crate) fn push_fx_default(&mut self, kind: FxKind) -> usize {
        let idx = self.fx.len();
        self.fx.push(FxSpec { kind, enabled: true });
        idx
    }

    pub(crate) fn fx_mut(&mut self) -> &mut Vec<FxSpec> {
        &mut self.fx
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(build: impl FnOnce(&mut SceneTrack)) -> TrackSpec {
        let mut t = SceneTrack::new();
        build(&mut t);
        t.into_spec()
    }

    fn key_of<F: 'static>(_f: F) -> String {
        track_key::<F>()
    }

    #[test]
    fn enabled_toggle_is_visible_in_the_diff() {
        // Defect 4: a static `.enabled(false)` must be a visible spec change.
        let a = spec(|t| {
            t.delay().enabled(true);
        });
        let b = spec(|t| {
            t.delay().enabled(false);
        });
        let (da, db) = (TrackDiff::of(&a), TrackDiff::of(&b));
        assert_ne!(da.fx, db.fx);
        assert!(fx_only_enabled_changed(&da.fx, &db.fx));
    }

    #[test]
    fn fx_param_change_is_not_enabled_only() {
        let a = spec(|t| {
            t.delay().feedback(0.2);
        });
        let b = spec(|t| {
            t.delay().feedback(0.5);
        });
        assert!(!fx_only_enabled_changed(&TrackDiff::of(&a).fx, &TrackDiff::of(&b).fx));
    }

    #[test]
    fn shape_records_an_automation() {
        // A `Shape` passed to a param seeds a static value and records an auto.
        use crate::music::shape::sine;
        let s = spec(|t| t.cutoff(sine(40.0).range(0.0, 1000.0)));
        assert_eq!(s.automations.len(), 1);
        assert_eq!(s.automations[0].0, ParamId::Cutoff);
        assert!((s.patch.cutoff - 500.0).abs() < 1e-2, "seeded from Clock::ZERO");
    }

    #[test]
    fn swing_is_recorded_on_the_spec() {
        let s = spec(|t| t.swing(0.25, 0.57));
        assert_eq!(s.swing, Some(Swing { grid: 0.25, amount: 0.57 }));
    }

    #[test]
    fn polyphony_change_is_seen() {
        // Defect 5: polyphony changes must reach the diff.
        let a = spec(|t| t.polyphony(4));
        let b = spec(|t| t.polyphony(8));
        assert_ne!(TrackDiff::of(&a).polyphony, TrackDiff::of(&b).polyphony);
    }

    mod bank_a {
        pub fn bass(_t: &mut super::super::SceneTrack) {}
    }
    mod bank_b {
        pub fn bass(_t: &mut super::super::SceneTrack) {}
    }

    #[test]
    fn same_named_tracks_in_different_modules_dont_collide() {
        // Defect 6: full module path is the engine key.
        let ka = key_of(bank_a::bass);
        let kb = key_of(bank_b::bass);
        assert_ne!(ka, kb, "distinct modules must map to distinct keys");
        assert_eq!(short_name(&ka), "bass");
        assert_eq!(short_name(&kb), "bass");
    }

    #[test]
    fn automation_closure_sets_initial_value_and_records_param() {
        let s = spec(|t| t.cutoff(|_c| 1234.0));
        assert_eq!(s.patch.cutoff, 1234.0, "initial value comes from Clock::ZERO");
        assert_eq!(s.automations.len(), 1);
        assert_eq!(s.automations[0].0, ParamId::Cutoff);
    }
}
