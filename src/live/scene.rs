//! The scene runtime: runs your `scene` function every frame, re-runs only the
//! track/bus/master functions whose code changed, diffs the resulting specs
//! against what the engine has, and sends just the differences.

use std::collections::{HashMap, HashSet};
use std::panic::{AssertUnwindSafe, catch_unwind};

use tracing::debug;

use crate::engine::bus::{BusChain, BusUpdate};
use crate::engine::chain::FxChain;
use crate::engine::command::{Command, EngineHandle};
use crate::engine::instrument::{InstrumentCfg, SampleSource, SourceCfg};
use crate::engine::track::{DuckCfg, PatternCfg, TrackBuild};
use crate::live::chain_builder::Chain;
use crate::live::track_builder::{Bus, Track};
use crate::model::chain::{ChainSpec, FxNode};
use crate::model::instrument::{InstrumentSpec, SourceSpec};
use crate::model::track::{BusId, BusSpec, DuckSpec, Route, TrackId, TrackSpec};
use crate::music::form::Section;
use crate::music::pitch::Key;
use crate::music::signal::{Program, Signal};
use crate::sample::SampleCache;

/// The diffable part of a track (everything but its pattern closure).
#[derive(Clone, PartialEq)]
struct TrackState {
    instrument: InstrumentSpec,
    gain: Signal,
    pan: Signal,
    mute: bool,
    chain: ChainSpec,
    route: Route,
    duck: Option<DuckSpec>,
    key: Option<Key>,
}

impl TrackState {
    fn of(spec: &TrackSpec) -> Self {
        TrackState {
            instrument: spec.instrument.clone(),
            gain: spec.gain.clone(),
            pan: spec.pan.clone(),
            mute: spec.mute,
            chain: spec.chain.clone(),
            route: spec.route.clone(),
            duck: spec.duck.clone(),
            key: spec.key,
        }
    }
}

#[cfg(feature = "hot-reload")]
type Memo = HashMap<String, subsecond::HotFnPtr>;
#[cfg(not(feature = "hot-reload"))]
type Memo = HashSet<String>;

/// Run a builder function if its code changed since it last ran (hot reload)
/// or if it never ran. Panics are caught and logged — a typo in one track
/// doesn't take the set down. Evaluates to `true` when it ran successfully.
macro_rules! rerun {
    ($memo:expr, $key:expr, $f:expr, $builder:expr) => {{
        let key: &str = $key;
        #[cfg(feature = "hot-reload")]
        let ran = {
            let ptr = subsecond::HotFn::current($f).ptr_address();
            if $memo.get(key) == Some(&ptr) {
                None
            } else {
                $memo.insert(key.to_string(), ptr);
                Some(catch_unwind(AssertUnwindSafe(|| subsecond::HotFn::current($f).call((&mut $builder,)))).is_ok())
            }
        };
        #[cfg(not(feature = "hot-reload"))]
        let ran = if $memo.contains(key) {
            None
        } else {
            $memo.insert(key.to_string());
            Some(catch_unwind(AssertUnwindSafe(|| ($f)(&mut $builder))).is_ok())
        };
        match ran {
            Some(false) => {
                tracing::error!("`{key}` panicked — keeping what was playing");
                false
            }
            Some(true) => true,
            None => false,
        }
    }};
}

pub struct Scene {
    handle: EngineHandle,
    sample_rate: f32,
    memo: Memo,
    samples: SampleCache,

    tracks: HashMap<TrackId, TrackState>,
    seen: HashSet<TrackId>,

    buses: HashMap<BusId, BusSpec>,
    bus_frame: Vec<BusId>,
    buses_sent: Vec<(BusId, BusSpec)>,

    master: ChainSpec,
    /// The master function's key: this frame's, and the last one seen.
    master_seen: Option<String>,
    master_key: Option<String>,
    master_sent: ChainSpec,

    tempo_sent: Option<f32>,
    key_sent: Option<Key>,
    jump_frame: Option<Section>,
    jump_prev: Option<Section>,
    hold_frame: Option<Section>,
    hold_sent: Option<Section>,
    midi_target: Option<TrackId>,
    midi_sent: Option<Option<String>>,
}

impl Scene {
    pub fn new(sample_rate: f32, handle: EngineHandle) -> Self {
        Self {
            handle,
            sample_rate,
            memo: Memo::default(),
            samples: SampleCache::new(),
            tracks: HashMap::new(),
            seen: HashSet::new(),
            buses: HashMap::new(),
            bus_frame: Vec::new(),
            buses_sent: Vec::new(),
            master: ChainSpec::default(),
            master_seen: None,
            master_key: None,
            master_sent: ChainSpec::default(),
            tempo_sent: None,
            key_sent: None,
            jump_frame: None,
            jump_prev: None,
            hold_frame: None,
            hold_sent: None,
            midi_target: None,
            midi_sent: None,
        }
    }

    // ── Global state ──

    pub fn tempo(&mut self, bpm: f32) {
        if self.tempo_sent != Some(bpm) {
            self.tempo_sent = Some(bpm);
            self.handle.send(Command::SetTempo(bpm));
        }
    }

    /// The key every track's patterns resolve degrees and numerals against.
    pub fn key(&mut self, root: u8, scale: &'static [u8]) {
        let key = Key::new(root, scale);
        if self.key_sent != Some(key) {
            self.key_sent = Some(key);
            self.handle.send(Command::SetKey(key));
        }
    }

    /// Jump the song to `section` at the next bar. Fires once when this line
    /// appears or its section changes.
    pub fn jump(&mut self, section: Section) {
        self.jump_frame = Some(section);
    }

    /// Loop `section` while this line is present; delete it to play on.
    pub fn hold(&mut self, section: Section) {
        self.hold_frame = Some(section);
    }

    /// Route MIDI keyboard input to a track.
    pub fn midi<F: Fn(&mut Track) + 'static>(&mut self, _f: F) {
        self.midi_target = Some(TrackId::of::<F>());
    }

    // ── Tracks ──

    /// Play a track. The function is its identity; on hot reload it re-runs
    /// only if its code changed, and only the differences reach the engine.
    pub fn track<F: Fn(&mut Track) + Copy + 'static>(&mut self, f: F) {
        let id = TrackId::of::<F>();
        self.seen.insert(id.clone());
        let mut builder = Track::new();
        if rerun!(self.memo, &id.key, f, builder) {
            self.apply_track(id, builder.into_spec());
        }
    }

    fn apply_track(&mut self, id: TrackId, mut spec: TrackSpec) {
        let state = TrackState::of(&spec);
        let key = id.key.clone();
        let pattern = spec.playback.take().map(|p| PatternCfg { func: p.pattern, len: p.len as f64, swing: spec.swing });

        let Some(prev) = self.tracks.get(&id).cloned() else {
            debug!(track = id.name(), "add");
            let build = TrackBuild {
                name: key,
                instrument: self.compile_instrument(&state.instrument),
                gain: Program::global(&state.gain),
                pan: Program::global(&state.pan),
                mute: state.mute,
                chain: FxChain::build(&state.chain, self.sample_rate),
                route: route_key(&state.route),
                duck: duck_cfg(&state.duck),
                key: state.key,
                pattern,
                seed: hash_key(&id.key),
            };
            self.handle.send(Command::AddTrack(Box::new(build)));
            self.tracks.insert(id, state);
            return;
        };

        if state.instrument != prev.instrument {
            debug!(track = id.name(), "instrument");
            let cfg = self.compile_instrument(&state.instrument);
            self.handle.send(Command::SetInstrument { track: key.clone(), cfg: Box::new(cfg) });
        }
        if (&state.gain, &state.pan, state.mute) != (&prev.gain, &prev.pan, prev.mute) {
            self.handle.send(Command::SetMixer {
                track: key.clone(),
                gain: Program::global(&state.gain),
                pan: Program::global(&state.pan),
                mute: state.mute,
            });
        }
        if state.chain != prev.chain {
            self.handle.send(Command::SetChain { track: key.clone(), updates: FxChain::diff(&prev.chain, &state.chain, self.sample_rate) });
        }
        if state.route != prev.route {
            self.handle.send(Command::SetRoute { track: key.clone(), route: route_key(&state.route) });
        }
        if state.duck != prev.duck {
            self.handle.send(Command::SetDuck { track: key.clone(), duck: duck_cfg(&state.duck) });
        }
        if state.key != prev.key {
            self.handle.send(Command::SetTrackKey { track: key.clone(), key: state.key });
        }
        // Pattern closures can't be compared: the builder re-ran because its
        // code changed, so queue the new pattern for the next loop boundary.
        if let Some(pattern) = pattern {
            self.handle.send(Command::QueuePattern { track: key, pattern });
        }
        self.tracks.insert(id, state);
    }

    fn compile_instrument(&mut self, spec: &InstrumentSpec) -> InstrumentCfg {
        let source = match &spec.source {
            SourceSpec::Osc(_) => SourceCfg::Osc(InstrumentCfg::oscs(spec)),
            SourceSpec::Sample { path, root } => match self.samples.get(path) {
                Some(data) => SourceCfg::Sample(SampleSource::Pitched { data, root: *root }),
                None => SourceCfg::Sample(SampleSource::Kit { map: HashMap::new() }),
            },
            SourceSpec::Kit(slots) => SourceCfg::Sample(SampleSource::Kit {
                map: slots.iter().filter_map(|(n, p)| self.samples.get(p).map(|d| (*n, d))).collect(),
            }),
        };
        InstrumentCfg::compile(spec, source)
    }

    // ── Buses and master ──

    /// Declare a bus. Tracks reach it with `t.to(bus)` or `c.send(bus, amt)`.
    pub fn bus<F: Fn(&mut Bus) + Copy + 'static>(&mut self, f: F) {
        let id = BusId::of::<F>();
        let mut builder = Bus::new();
        if rerun!(self.memo, &id.key, f, builder) {
            self.buses.insert(id.clone(), builder.into_spec());
        }
        if !self.bus_frame.contains(&id) {
            self.bus_frame.push(id);
        }
    }

    /// The master chain, applied to the final mix.
    pub fn master<F: Fn(&mut Chain) + Copy + 'static>(&mut self, f: F) {
        let key = std::any::type_name::<F>();
        self.master_seen = Some(key.to_string());
        let mut chain = Chain::new();
        if rerun!(self.memo, key, f, chain) {
            self.master = chain.into_spec();
        }
    }

    /// Close the frame: remove tracks and buses no longer mentioned, ship bus
    /// and master changes, and apply edge-triggered arrangement commands.
    pub fn finish_frame(&mut self) {
        let removed: Vec<TrackId> = self.tracks.keys().filter(|id| !self.seen.contains(id)).cloned().collect();
        for id in removed {
            debug!(track = id.name(), "remove");
            self.tracks.remove(&id);
            self.memo.remove(&id.key);
            self.handle.send(Command::RemoveTrack(id.key));
        }
        self.seen.clear();

        self.finish_buses();

        let seen = self.master_seen.take();
        if seen != self.master_key {
            // The master function changed or was deleted: forget the old one so
            // it re-runs if it comes back.
            if let Some(old) = self.master_key.take() {
                self.memo.remove(&old);
            }
            if seen.is_none() {
                self.master = ChainSpec::default();
            }
            self.master_key = seen;
        }
        if self.master != self.master_sent {
            self.handle.send(Command::SetMaster(FxChain::diff(&self.master_sent, &self.master, self.sample_rate)));
            self.master_sent = self.master.clone();
        }

        if self.jump_frame.is_some() && self.jump_frame != self.jump_prev {
            let s = self.jump_frame.expect("checked");
            self.handle.send(Command::Jump(s.start_beat()));
        }
        self.jump_prev = self.jump_frame.take();

        let hold = self.hold_frame.take();
        if hold != self.hold_sent {
            self.handle.send(Command::Hold(hold.map(|s| (s.start_beat(), s.end_beat()))));
            self.hold_sent = hold;
        }

        let target = self.midi_target.take().map(|id| id.key);
        if self.midi_sent.as_ref() != Some(&target) {
            self.handle.send(Command::SetMidiTrack(target.clone()));
            self.midi_sent = Some(target);
        }
    }

    fn finish_buses(&mut self) {
        let frame = std::mem::take(&mut self.bus_frame);
        self.buses.retain(|id, _| frame.contains(id));
        for (id, _) in &self.buses_sent {
            if !frame.contains(id) {
                self.memo.remove(&id.key);
            }
        }
        let current: Vec<(BusId, BusSpec)> = order_buses(&frame, &self.buses);
        if current == self.buses_sent {
            return;
        }
        let updates = current
            .iter()
            .map(|(id, spec)| {
                let chain = match self.buses_sent.iter().find(|(p, _)| p == id) {
                    Some((_, prev)) => BusChain::Update(FxChain::diff(&prev.chain, &spec.chain, self.sample_rate)),
                    None => BusChain::New(FxChain::build(&spec.chain, self.sample_rate)),
                };
                BusUpdate { name: id.key.clone(), gain: Program::global(&spec.gain), mute: spec.mute, route: route_key(&spec.route), chain }
            })
            .collect();
        self.handle.send(Command::SetBuses(updates));
        self.buses_sent = current;
    }
}

/// Order buses so each precedes every bus it routes or sends to (Kahn's
/// algorithm, stable in declaration order). Cycles are broken by declaration
/// order; the engine then drops the backwards edges.
fn order_buses(frame: &[BusId], specs: &HashMap<BusId, BusSpec>) -> Vec<(BusId, BusSpec)> {
    let ids: Vec<&BusId> = frame.iter().filter(|id| specs.contains_key(id)).collect();
    let targets = |id: &BusId| -> Vec<BusId> {
        let spec = &specs[id];
        let mut out = Vec::new();
        if let Route::Bus(b) = &spec.route {
            out.push(b.clone());
        }
        collect_sends(&spec.chain, &mut out);
        out
    };
    let mut indegree: HashMap<&BusId, usize> = ids.iter().map(|id| (*id, 0)).collect();
    for id in &ids {
        for t in targets(id) {
            if let Some(d) = indegree.get_mut(&t) {
                *d += 1;
            }
        }
    }
    let mut out = Vec::new();
    let mut placed: HashSet<&BusId> = HashSet::new();
    while out.len() < ids.len() {
        let next = ids.iter().find(|id| !placed.contains(*id) && indegree[*id] == 0).or_else(|| {
            tracing::error!("bus routing has a cycle");
            ids.iter().find(|id| !placed.contains(*id))
        });
        let Some(id) = next else { break };
        placed.insert(id);
        for t in targets(id) {
            if let Some(d) = indegree.get_mut(&t) {
                *d = d.saturating_sub(1);
            }
        }
        out.push(((*id).clone(), specs[*id].clone()));
    }
    out
}

fn collect_sends(chain: &ChainSpec, out: &mut Vec<BusId>) {
    for node in &chain.0 {
        match node {
            FxNode::Send { bus, .. } => out.push(bus.clone()),
            FxNode::Parallel(branches) => branches.iter().for_each(|b| collect_sends(b, out)),
            FxNode::Fx { .. } => {}
        }
    }
}

fn route_key(route: &Route) -> Option<String> {
    match route {
        Route::Master => None,
        Route::Bus(b) => Some(b.key.clone()),
    }
}

fn duck_cfg(duck: &Option<DuckSpec>) -> Option<DuckCfg> {
    duck.as_ref().map(|d| DuckCfg { source: d.source.key.clone(), depth: d.depth, release: d.release })
}

/// FNV-1a hash of the track key — a stable per-track RNG seed.
fn hash_key(key: &str) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for b in key.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}
