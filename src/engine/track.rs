//! A running track: instrument, scheduler, chain, mixer, sidechain duck.
//! Each control block it is first *prepared* (signals evaluated, pattern
//! regenerated, events collected) and then *rendered*; the split lets the
//! engine route duck triggers between tracks in between.

use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::dsp::StereoFrame;
use crate::dsp::effects::FxCtx;
use crate::dsp::smooth::Smoothed;
use crate::engine::CONTROL_BLOCK;
use crate::engine::bus::Bus;
use crate::engine::chain::{FxChain, NodeUpdate};
use crate::engine::instrument::{Instrument, InstrumentCfg, ModCtx};
use crate::engine::mixer::pan_mono;
use crate::engine::scheduler::{Fired, NoteEv, Scheduler};
use crate::engine::transport::Transport;
use crate::model::track::{PatternFn, Swing};
use crate::music::phrase::{Event, Phrase, PhraseCtx};
use crate::music::pitch::Key;
use crate::music::signal::{EvalCtx, Program, VoiceInputs};

/// Sidechain settings, resolved to engine keys.
#[derive(Clone, Debug, PartialEq)]
pub struct DuckCfg {
    pub source: String,
    pub depth: f32,
    /// Recovery time in beats.
    pub release: f32,
}

pub struct PatternCfg {
    pub func: PatternFn,
    pub len: f64,
    pub swing: Option<Swing>,
}

/// Everything needed to construct a running track, built on the control thread.
pub struct TrackBuild {
    pub name: String,
    pub instrument: InstrumentCfg,
    pub gain: Program,
    pub pan: Program,
    pub mute: bool,
    pub chain: FxChain,
    pub route: Option<String>,
    pub duck: Option<DuckCfg>,
    pub key: Option<Key>,
    pub pattern: Option<PatternCfg>,
    /// Deterministic RNG seed (hash of the track key).
    pub seed: u64,
}

/// Shared context for one control block.
pub struct BlockCtx<'a> {
    /// Engine beats at the block's start and end.
    pub b0: f64,
    pub b1: f64,
    pub bps: f64,
    pub transport: &'a Transport,
    pub knobs: &'a [f32; 128],
    pub key: Key,
    pub fx: FxCtx,
}

struct Duck {
    cfg: DuckCfg,
    /// Beats since the last trigger.
    since: f64,
    triggers: Vec<usize>,
}

impl Duck {
    const ATTACK_SECONDS: f64 = 0.003;

    fn new(cfg: DuckCfg) -> Self {
        Duck { cfg, since: f64::MAX, triggers: Vec::new() }
    }

    /// Gain multiplier for the next sample.
    #[inline]
    fn next(&mut self, attack: f64, bps: f64) -> f32 {
        let t = self.since;
        let shape = if t < attack {
            t / attack
        } else {
            let x = (1.0 - (t - attack) / self.cfg.release.max(1e-3) as f64).max(0.0);
            x * x
        };
        self.since += bps;
        1.0 - self.cfg.depth.clamp(0.0, 1.0) * shape as f32
    }
}

pub struct Track {
    pub name: String,
    pub route: Option<String>,
    instrument: Instrument,
    scheduler: Scheduler,
    chain: FxChain,
    gain: Program,
    pan: Program,
    gain_s: Smoothed,
    pan_s: Smoothed,
    mute: bool,
    duck: Option<Duck>,
    key: Option<Key>,
    pattern: Option<PatternFn>,
    loop_len: Option<f64>,
    swing: Option<Swing>,
    pending: Option<PatternCfg>,
    seed: u64,
    mono: Vec<f32>,
    out: Vec<StereoFrame>,
    events: Vec<Fired>,
    onsets: Vec<usize>,
}

impl Track {
    /// Build a track and launch its pattern at engine beat `now`.
    pub fn new(b: TrackBuild, now: f64, transport: &Transport, knobs: &[f32; 128], key: Key, sample_rate: f32) -> Self {
        let mut t = Track {
            name: b.name,
            route: b.route,
            instrument: Instrument::new(b.instrument, sample_rate),
            scheduler: Scheduler::new(),
            chain: b.chain,
            gain: b.gain,
            pan: b.pan,
            gain_s: Smoothed::new(0.0, 8.0, sample_rate),
            pan_s: Smoothed::new(0.0, 8.0, sample_rate),
            mute: b.mute,
            duck: b.duck.map(Duck::new),
            key: b.key,
            pattern: None,
            loop_len: None,
            swing: None,
            pending: None,
            seed: b.seed,
            mono: vec![0.0; CONTROL_BLOCK],
            out: vec![[0.0; 2]; CONTROL_BLOCK],
            events: Vec::with_capacity(64),
            onsets: Vec::with_capacity(16),
        };
        let eval = |p: &Program| p.eval(&EvalCtx { beat: transport.song(now), knobs, voice: &VoiceInputs::default() });
        t.gain_s.snap(eval(&t.gain));
        t.pan_s.snap(eval(&t.pan));
        if let Some(p) = b.pattern {
            t.launch(p, now, transport, knobs, key);
        }
        t
    }

    fn launch(&mut self, p: PatternCfg, now: f64, transport: &Transport, knobs: &[f32; 128], key: Key) {
        let PatternCfg { mut func, len, swing } = p;
        let mut ons = self.generate(&mut func, len, now, 0, transport, knobs, key).unwrap_or_default();
        apply_swing(&mut ons, swing);
        self.pattern = Some(func);
        self.loop_len = Some(len);
        self.swing = swing;
        self.scheduler.launch(ons, Some(len), now);
    }

    /// Run the pattern closure for the loop starting at engine beat `at`.
    /// Panics are caught: a broken edit keeps the previous loop playing.
    #[allow(clippy::too_many_arguments)]
    fn generate(&self, func: &mut PatternFn, len: f64, at: f64, iteration: u32, transport: &Transport, knobs: &[f32; 128], key: Key) -> Option<Vec<Event>> {
        let song = transport.song(at);
        let cycle = (song / len.max(1e-6)).floor().max(0.0) as u32;
        let ctx = PhraseCtx {
            beat: song,
            len: len as f32,
            cycle,
            iteration,
            seed: self.seed ^ mix(cycle),
            key: self.key.unwrap_or(key),
            knobs: *knobs,
        };
        match catch_unwind(AssertUnwindSafe(|| {
            let mut phrase = Phrase::new(ctx);
            func(&mut phrase);
            phrase.into_events()
        })) {
            Ok(events) => Some(events),
            Err(_) => {
                tracing::error!(track = %self.name, "pattern panicked — keeping the previous loop");
                None
            }
        }
    }

    // ── Edits ──

    pub fn set_instrument(&mut self, cfg: InstrumentCfg) {
        self.instrument.set_cfg(cfg);
    }

    pub fn set_mixer(&mut self, gain: Program, pan: Program, mute: bool) {
        self.gain = gain;
        self.pan = pan;
        self.mute = mute;
    }

    pub fn set_chain(&mut self, updates: Vec<NodeUpdate>) {
        self.chain.apply(updates);
    }

    pub fn set_duck(&mut self, duck: Option<DuckCfg>) {
        match (&mut self.duck, duck) {
            (Some(d), Some(cfg)) => d.cfg = cfg,
            (slot, cfg) => *slot = cfg.map(Duck::new),
        }
    }

    pub fn set_key(&mut self, key: Option<Key>) {
        self.key = key;
    }

    /// Replace the pattern at the next loop boundary.
    pub fn queue_pattern(&mut self, p: PatternCfg) {
        self.pending = Some(p);
    }

    pub fn note_on(&mut self, note: u8, vel: f32) {
        self.instrument.note_on(note, vel, false, Default::default());
    }

    pub fn note_off(&mut self, note: u8) {
        self.instrument.note_off(note);
    }

    // ── Per-block ──

    pub fn duck_source(&self) -> Option<&str> {
        self.duck.as_ref().map(|d| d.cfg.source.as_str())
    }

    /// Sample offsets of this block's note-ons.
    pub fn onsets(&self) -> &[usize] {
        &self.onsets
    }

    pub fn set_duck_triggers(&mut self, offsets: &[usize]) {
        if let Some(d) = &mut self.duck {
            d.triggers.clear();
            d.triggers.extend_from_slice(offsets);
        }
    }

    pub fn out(&self, n: usize) -> &[StereoFrame] {
        &self.out[..n]
    }

    /// Evaluate signals, apply queued timing changes, regenerate, and collect
    /// this block's events.
    pub fn prepare(&mut self, ctx: &BlockCtx, n: usize) {
        let voice = VoiceInputs::default();
        let eval = EvalCtx { beat: ctx.fx.beat, knobs: ctx.knobs, voice: &voice };
        self.gain_s.set_target(self.gain.eval(&eval).max(0.0));
        self.pan_s.set_target(self.pan.eval(&eval).clamp(-1.0, 1.0));

        self.apply_pending(ctx);
        self.maybe_regenerate(ctx);

        self.events.clear();
        self.scheduler.collect(ctx.b0, ctx.b1, &mut self.events);
        self.onsets.clear();
        for ev in &self.events {
            if matches!(ev.ev, NoteEv::On { .. }) {
                self.onsets.push(offset(ev.beat, ctx.b0, ctx.bps, n));
            }
        }
    }

    fn apply_pending(&mut self, ctx: &BlockCtx) {
        let Some(PatternCfg { mut func, len, swing }) = self.pending.take() else {
            return;
        };
        if !self.scheduler.is_looping() {
            self.launch(PatternCfg { func, len, swing }, ctx.b0, ctx.transport, ctx.knobs, ctx.key);
            return;
        }
        let boundary = self.scheduler.next_boundary().unwrap_or(ctx.b0);
        let next = self.scheduler.iteration() + 1;
        let mut ons = self.generate(&mut func, len, boundary, next, ctx.transport, ctx.knobs, ctx.key).unwrap_or_default();
        apply_swing(&mut ons, swing);
        self.pattern = Some(func);
        self.loop_len = Some(len);
        self.swing = swing;
        self.scheduler.queue(ons, Some(len), true, ctx.b0);
    }

    fn maybe_regenerate(&mut self, ctx: &BlockCtx) {
        if self.scheduler.has_queued() {
            return;
        }
        let (Some(boundary), Some(len)) = (self.scheduler.next_boundary(), self.loop_len) else {
            return;
        };
        if boundary < ctx.b0 || boundary >= ctx.b1 {
            return;
        }
        let next = self.scheduler.iteration() + 1;
        let Some(mut func) = self.pattern.take() else {
            return;
        };
        let generated = self.generate(&mut func, len, boundary, next, ctx.transport, ctx.knobs, ctx.key);
        self.pattern = Some(func);
        if let Some(mut ons) = generated {
            apply_swing(&mut ons, self.swing);
            self.scheduler.queue(ons, Some(len), false, ctx.b0);
        }
    }

    /// Render `n` frames into this track's output buffer: instrument (split at
    /// each event's sample offset) → gain × duck → pan → chain.
    pub fn render(&mut self, ctx: &BlockCtx, n: usize, buses: &mut [Bus]) {
        let song0 = ctx.fx.beat;
        let mono = &mut self.mono[..n];
        let mut pos = 0;
        for ev in &self.events {
            let off = offset(ev.beat, ctx.b0, ctx.bps, n);
            if off > pos {
                let m = ModCtx { beat: song0 + pos as f64 * ctx.bps, bps: ctx.bps, knobs: ctx.knobs };
                self.instrument.render(&mut mono[pos..off], &m);
                pos = off;
            }
            match ev.ev {
                NoteEv::On { note, vel, slide, locks } => self.instrument.note_on(note, vel, slide, locks),
                NoteEv::Off { note } => self.instrument.note_off(note),
            }
        }
        if pos < n {
            let m = ModCtx { beat: song0 + pos as f64 * ctx.bps, bps: ctx.bps, knobs: ctx.knobs };
            self.instrument.render(&mut mono[pos..n], &m);
        }

        let attack = Duck::ATTACK_SECONDS * ctx.fx.sample_rate as f64 * ctx.bps;
        let mut trigger = 0;
        let mute = if self.mute { 0.0 } else { 1.0 };
        for (i, frame) in self.out[..n].iter_mut().enumerate() {
            let mut g = self.gain_s.next() * mute;
            if let Some(d) = &mut self.duck {
                while trigger < d.triggers.len() && d.triggers[trigger] <= i {
                    d.since = 0.0;
                    trigger += 1;
                }
                g *= d.next(attack, ctx.bps);
            }
            *frame = pan_mono(mono[i] * g, self.pan_s.next());
        }
        self.chain.process(&mut self.out[..n], &ctx.fx, ctx.knobs, buses);
    }
}

#[inline]
fn offset(beat: f64, b0: f64, bps: f64, n: usize) -> usize {
    (((beat - b0) / bps).round() as isize).clamp(0, n as isize) as usize
}

/// SplitMix64 finalizer — decorrelates successive cycles before they mix into
/// the seed.
fn mix(cycle: u32) -> u64 {
    let mut z = (cycle as u64).wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Shift notes on odd multiples of the swing grid later. Applied in exactly one
/// place — where phrase output becomes scheduler note-ons.
pub fn apply_swing(notes: &mut [Event], swing: Option<Swing>) {
    let Some(Swing { grid, amount }) = swing else {
        return;
    };
    if grid <= 0.0 {
        return;
    }
    let shift = (amount - 0.5) * 2.0 * grid;
    for n in notes {
        let k = (n.beat / grid).round();
        if (n.beat - k * grid).abs() < 1e-4 && (k as i64).rem_euclid(2) == 1 {
            n.beat += shift;
        }
    }
}
