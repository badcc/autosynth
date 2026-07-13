use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::dsp::effects::Effect;
use crate::dsp::smooth::Smoothed;
use crate::engine::instrument::{Instrument, SampleSource};
use crate::engine::mixer::pan_mono;
use crate::engine::scheduler::{Fired, NoteEv, Scheduler};
use crate::dsp::StereoFrame;
use crate::model::param::{Automation, ParamId, PatternFn};
use crate::model::PatchSpec;
use crate::music::{NoteSpec, Phrase};

/// A timing change queued by a hot-reload, applied at the next loop boundary
/// (or immediately if the track isn't looping).
pub enum Pending {
    Pattern { func: PatternFn, loop_len: f64 },
    OneShot(Vec<NoteSpec>),
}

/// A running track: a sound generator, a scheduler, an fx chain, and mixer
/// state. Renders one control block at a time.
pub struct Track {
    pub instrument: Instrument,
    pub scheduler: Scheduler,
    pub fx: Vec<Box<dyn Effect>>,
    pub fx_enabled: Vec<bool>,
    pub gain: Smoothed,
    pub pan: f32,
    pub mute: bool,
    automations: Vec<Automation>,
    pattern: Option<PatternFn>,
    loop_len: Option<f64>,
    pending: Option<Pending>,
    sample_rate: f32,
    // Preallocated scratch, reused every block (no per-block allocation).
    mono: Vec<f32>,
    events: Vec<Fired>,
}

impl Track {
    fn from_instrument(instrument: Instrument, gain: f32, pan: f32, sample_rate: f32) -> Self {
        Self {
            instrument,
            scheduler: Scheduler::new(),
            fx: Vec::new(),
            fx_enabled: Vec::new(),
            gain: Smoothed::new(gain, 8.0, sample_rate),
            pan,
            mute: false,
            automations: Vec::new(),
            pattern: None,
            loop_len: None,
            pending: None,
            sample_rate,
            mono: Vec::new(),
            events: Vec::new(),
        }
    }

    pub fn new_synth(
        sample_rate: f32,
        polyphony: usize,
        patch: &PatchSpec,
        gain: f32,
        pan: f32,
    ) -> Self {
        Self::from_instrument(
            Instrument::synth(sample_rate, polyphony, patch),
            gain,
            pan,
            sample_rate,
        )
    }

    pub fn new_sampler(
        sample_rate: f32,
        polyphony: usize,
        patch: &PatchSpec,
        gain: f32,
        pan: f32,
        source: SampleSource,
    ) -> Self {
        Self::from_instrument(
            Instrument::sampler(sample_rate, polyphony, patch, source),
            gain,
            pan,
            sample_rate,
        )
    }

    // ── Sound-speed updates (apply immediately) ──

    pub fn apply_patch(&mut self, patch: &PatchSpec) {
        self.instrument.apply_patch(patch);
    }

    pub fn set_mixer(&mut self, gain: f32, pan: f32, mute: bool) {
        self.gain.set_target(gain);
        self.pan = pan;
        self.mute = mute;
    }

    pub fn set_fx(&mut self, fx: Vec<Box<dyn Effect>>, enabled: Vec<bool>) {
        self.fx = fx;
        self.fx_enabled = enabled;
    }

    pub fn set_fx_enabled(&mut self, enabled: Vec<bool>) {
        self.fx_enabled = enabled;
    }

    pub fn set_automations(&mut self, automations: Vec<Automation>) {
        self.automations = automations;
    }

    // ── Timing-speed updates (queue to boundary) ──

    /// Launch a pattern immediately (used on track creation).
    pub fn launch_pattern(&mut self, mut func: PatternFn, loop_len: f64, now: f64) {
        let ons = run_pattern(&mut func, now as f32, 0).unwrap_or_default();
        self.pattern = Some(func);
        self.loop_len = Some(loop_len);
        self.scheduler.launch(ons, Some(loop_len), now);
    }

    /// Launch one-shot notes immediately (used on track creation).
    pub fn launch_oneshot(&mut self, notes: Vec<NoteSpec>, now: f64) {
        self.pattern = None;
        self.loop_len = None;
        self.scheduler.launch(notes, None, now);
    }

    /// Queue a pattern swap for the next loop boundary.
    pub fn queue_pattern(&mut self, func: PatternFn, loop_len: f64) {
        self.pending = Some(Pending::Pattern { func, loop_len });
    }

    /// Queue a one-shot phrase swap for the next loop boundary.
    pub fn queue_oneshot(&mut self, notes: Vec<NoteSpec>) {
        self.pending = Some(Pending::OneShot(notes));
    }

    pub fn stop(&mut self) {
        self.scheduler.stop();
    }

    // ── MIDI (real-time direct triggers) ──

    pub fn note_on(&mut self, note: u8, vel: f32) {
        self.instrument.note_on(note, vel);
    }

    pub fn note_off(&mut self, note: u8) {
        self.instrument.note_off(note);
    }

    // ── Rendering ──

    /// Render one control block, summing this track's stereo output into `out`.
    /// `b0`/`b1` are the global beats at the block's start/end; `bps` is beats
    /// per sample.
    pub fn render_add(&mut self, out: &mut [StereoFrame], b0: f64, b1: f64, bps: f64) {
        let n = out.len();
        if n == 0 {
            return;
        }

        // 1. Control-rate automation.
        self.apply_automations(b0);

        // 2. Apply any queued hot-reload timing update.
        self.apply_pending(b0);

        // 3. Regenerate the upcoming iteration just before a boundary.
        self.maybe_regenerate(b0, b1);

        // 4. Collect note events for this block.
        let mut events = std::mem::take(&mut self.events);
        events.clear();
        self.scheduler.collect(b0, b1, &mut events);

        // 5. Render mono, splitting at each event's sample offset.
        let mut mono = std::mem::take(&mut self.mono);
        mono.clear();
        mono.resize(n, 0.0);
        let mut pos = 0usize;
        for ev in &events {
            let off = (((ev.beat - b0) / bps).round() as isize).clamp(0, n as isize) as usize;
            if off > pos {
                self.instrument.render_block(&mut mono[pos..off]);
                pos = off;
            }
            match ev.ev {
                NoteEv::On { note, vel } => self.instrument.note_on(note, vel),
                NoteEv::Off { note } => self.instrument.note_off(note),
            }
        }
        if pos < n {
            self.instrument.render_block(&mut mono[pos..n]);
        }

        // 6. Widen to stereo, run fx, apply mixer gain, sum into out.
        let sr = self.sample_rate;
        let mute = self.mute;
        for (i, frame) in out.iter_mut().enumerate() {
            let mut f = pan_mono(mono[i], self.pan);
            for (fx, en) in self.fx.iter_mut().zip(self.fx_enabled.iter()) {
                if *en {
                    f = fx.process(f, sr);
                }
            }
            let g = self.gain.next() * if mute { 0.0 } else { 1.0 };
            frame[0] += f[0] * g;
            frame[1] += f[1] * g;
        }

        self.mono = mono;
        self.events = events;
    }

    fn apply_automations(&mut self, b0: f64) {
        if self.automations.is_empty() {
            return;
        }
        let clock = self.scheduler.clock_at(b0);
        let mut autos = std::mem::take(&mut self.automations);
        for (id, f) in &mut autos {
            let v = f(clock);
            match *id {
                ParamId::Attack => self.instrument.params_mut().attack = v.max(0.0),
                ParamId::Decay => self.instrument.params_mut().decay = v.max(0.0),
                ParamId::Sustain => self.instrument.params_mut().sustain = v.clamp(0.0, 1.0),
                ParamId::Release => self.instrument.params_mut().release = v.max(0.0),
                ParamId::Cutoff => self
                    .instrument
                    .params_mut()
                    .cutoff
                    .set_target(v.clamp(20.0, 20000.0)),
                ParamId::Resonance => {
                    self.instrument.params_mut().resonance = v.clamp(0.0, 1.0)
                }
                ParamId::LfoRate => self.instrument.params_mut().lfo_rate = v.clamp(0.01, 20.0),
                ParamId::LfoDepth => self.instrument.params_mut().lfo_depth = v.clamp(0.0, 1.0),
                ParamId::Gain => self.gain.set_target(v.max(0.0)),
                ParamId::Pan => self.pan = v.clamp(-1.0, 1.0),
                ParamId::OscLevel(idx) => self.instrument.set_osc_level(idx, v),
                ParamId::OscDetune(idx) => self.instrument.set_osc_detune(idx, v),
                ParamId::Fx { index, slot } => {
                    if let Some(fx) = self.fx.get_mut(index) {
                        fx.set_param(slot, v);
                    }
                }
                ParamId::FxEnabled(idx) => {
                    if let Some(e) = self.fx_enabled.get_mut(idx) {
                        *e = v > 0.5;
                    }
                }
            }
        }
        self.automations = autos;
    }

    fn apply_pending(&mut self, b0: f64) {
        let Some(pending) = self.pending.take() else {
            return;
        };
        if self.scheduler.is_looping() {
            // Install as a choking swap at the next boundary.
            let boundary = self.scheduler.next_boundary().unwrap_or(b0);
            let next_it = self.scheduler.iteration() + 1;
            match pending {
                Pending::Pattern { mut func, loop_len } => {
                    let ons = run_pattern(&mut func, boundary as f32, next_it).unwrap_or_default();
                    self.pattern = Some(func);
                    self.loop_len = Some(loop_len);
                    self.scheduler.queue(ons, Some(loop_len), true, b0);
                }
                Pending::OneShot(notes) => {
                    self.pattern = None;
                    self.loop_len = None;
                    self.scheduler.queue(notes, None, true, b0);
                }
            }
        } else {
            // Nothing to wait for — relaunch now.
            match pending {
                Pending::Pattern { func, loop_len } => {
                    self.launch_pattern(func, loop_len, b0);
                }
                Pending::OneShot(notes) => {
                    self.launch_oneshot(notes, b0);
                }
            }
        }
    }

    fn maybe_regenerate(&mut self, b0: f64, b1: f64) {
        if self.scheduler.has_queued() {
            return; // A swap is already staged for the next boundary.
        }
        let Some(boundary) = self.scheduler.next_boundary() else {
            return;
        };
        if boundary < b0 || boundary >= b1 {
            return;
        }
        let next_it = self.scheduler.iteration() + 1;
        let loop_len = self.loop_len;
        if let Some(pat) = &mut self.pattern
            && let Some(ons) = run_pattern(pat, boundary as f32, next_it)
        {
            self.scheduler.queue(ons, loop_len, false, b0);
        }
    }
}

/// Run a pattern closure, catching panics so a broken edit keeps the previous
/// loop playing instead of killing the audio thread.
fn run_pattern(func: &mut PatternFn, beat: f32, iteration: u32) -> Option<Vec<NoteSpec>> {
    let result = catch_unwind(AssertUnwindSafe(|| {
        let mut phrase = Phrase::with_context(beat, iteration);
        func(&mut phrase);
        phrase.into_notes()
    }));
    match result {
        Ok(notes) => Some(notes),
        Err(_) => {
            tracing::error!("pattern closure panicked — keeping previous loop");
            None
        }
    }
}
