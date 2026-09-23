//! The track and bus builders — the surface a track function writes against:
//!
//! ```ignore
//! fn bass(t: &mut Track) {
//!     t.synth(presets::acid());
//!     t.duck(kick).depth(0.6);
//!     t.play(bars(2), |p| { p.seq("1 1 8 1 b3 1 . 5"); });
//!     t.fx(|c| { c.drive(3.0); });
//! }
//! ```

use std::path::PathBuf;

use crate::live::chain_builder::Chain;
use crate::model::instrument::{InstrumentSpec, Sampler, SourceSpec, Synth};
use crate::model::track::{BusId, BusSpec, DuckSpec, Playback, Route, Swing, TrackId, TrackSpec};
use crate::music::notes::C1;
use crate::music::phrase::Phrase;
use crate::music::pitch::Key;
use crate::music::signal::Signal;

pub struct Track {
    spec: TrackSpec,
    /// Kit voice settings from `t.sampler(Sampler::kit()..)`.
    kit: Option<InstrumentSpec>,
    slots: Vec<(u8, PathBuf)>,
    next_slot: u8,
}

impl Track {
    pub(crate) fn new() -> Self {
        Track { spec: TrackSpec::default(), kit: None, slots: Vec::new(), next_slot: C1 }
    }

    pub(crate) fn into_spec(mut self) -> TrackSpec {
        if !self.slots.is_empty() || self.kit.is_some() {
            let kit = self.kit.unwrap_or_else(|| Sampler::kit().into());
            self.spec.instrument = kit.with_slots(self.slots);
        }
        self.spec
    }

    // ── Sound source ──

    pub fn synth(&mut self, s: Synth) {
        self.spec.instrument = s.into();
    }

    /// A pitched sampler, or kit voice settings (`Sampler::kit()`) for the
    /// slots added with [`Track::slot`].
    pub fn sampler(&mut self, s: Sampler) {
        let spec: InstrumentSpec = s.into();
        match spec.source {
            SourceSpec::Kit(_) => self.kit = Some(spec),
            _ => self.spec.instrument = spec,
        }
    }

    /// Add a drum-kit sample; returns the note that plays it.
    pub fn slot(&mut self, path: impl Into<PathBuf>) -> u8 {
        let note = self.next_slot;
        self.slot_at(note, path)
    }

    /// Add a drum-kit sample on a specific note.
    pub fn slot_at(&mut self, note: u8, path: impl Into<PathBuf>) -> u8 {
        self.next_slot = self.next_slot.max(note.saturating_add(1));
        self.slots.push((note, path.into()));
        note
    }

    // ── Mixer ──

    /// Track gain (before the chain, so sends follow it).
    pub fn gain(&mut self, v: impl Into<Signal>) {
        self.spec.gain = v.into();
    }

    /// Pan, `-1..1`.
    pub fn pan(&mut self, v: impl Into<Signal>) {
        self.spec.pan = v.into();
    }

    pub fn mute(&mut self, on: bool) {
        self.spec.mute = on;
    }

    /// Append effects: a closure or a chain function (`t.fx(chains::space)`).
    pub fn fx(&mut self, f: impl FnOnce(&mut Chain)) {
        let spec = Chain::from_fn(f);
        self.spec.chain.0.extend(spec.0);
    }

    /// Output to a bus instead of the master.
    pub fn to<F: Fn(&mut Bus) + 'static>(&mut self, _bus: F) {
        self.spec.route = Route::Bus(BusId::of::<F>());
    }

    /// Sidechain: dip this track's gain on every note of `source`.
    pub fn duck<F: Fn(&mut Track) + 'static>(&mut self, _source: F) -> DuckKnobs<'_> {
        self.spec.duck = Some(DuckSpec { source: TrackId::of::<F>(), depth: 0.6, release: 0.5 });
        DuckKnobs(self.spec.duck.as_mut().expect("just set"))
    }

    // ── Patterns ──

    /// Override the scene key for this track.
    pub fn key(&mut self, root: u8, scale: &'static [u8]) {
        self.spec.key = Some(Key::new(root, scale));
    }

    /// Swing notes on odd multiples of `grid`; `0.5` is straight.
    pub fn swing(&mut self, grid: f32, amount: f32) {
        self.spec.swing = Some(Swing { grid, amount });
    }

    /// Loop a pattern `len` beats long. The closure re-runs every loop.
    pub fn play(&mut self, len: f32, f: impl FnMut(&mut Phrase) + Send + 'static) {
        self.spec.playback = Some(Playback { len, pattern: Box::new(f) });
    }
}

/// Sidechain settings.
pub struct DuckKnobs<'a>(&'a mut DuckSpec);

impl DuckKnobs<'_> {
    /// How far the gain dips, `0..1`.
    pub fn depth(self, d: f32) -> Self {
        self.0.depth = d;
        self
    }

    /// Recovery time in beats.
    pub fn release(self, beats: f32) -> Self {
        self.0.release = beats;
        self
    }
}

/// A bus builder: `fn space(b: &mut Bus) { b.fx(chains::space); }`.
#[derive(Default)]
pub struct Bus {
    spec: BusSpec,
}

impl Bus {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn into_spec(self) -> BusSpec {
        self.spec
    }

    /// Fader, after the chain.
    pub fn gain(&mut self, v: impl Into<Signal>) {
        self.spec.gain = v.into();
    }

    pub fn mute(&mut self, on: bool) {
        self.spec.mute = on;
    }

    pub fn fx(&mut self, f: impl FnOnce(&mut Chain)) {
        let spec = Chain::from_fn(f);
        self.spec.chain.0.extend(spec.0);
    }

    /// Output to another bus instead of the master.
    pub fn to<F: Fn(&mut Bus) + 'static>(&mut self, _bus: F) {
        self.spec.route = Route::Bus(BusId::of::<F>());
    }
}
