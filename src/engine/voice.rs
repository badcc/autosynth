use crate::dsp::envelope::{Adsr, RetriggerMode};
use crate::dsp::filter::{FilterType, Svf};
use crate::dsp::smooth::Smoothed;

/// The one thing that differs between instruments: how a triggered note turns
/// into a raw (pre-envelope, pre-filter) mono signal. Everything else — voice
/// allocation, stealing, envelope, filter, LFO, param handling — is shared in
/// `VoiceBank`.
pub trait Source: Send + Sized {
    /// Shared, read-only-at-render-time configuration (osc list / sample source).
    type Cfg: Send;

    fn new(sample_rate: f32, cfg: &Self::Cfg) -> Self;
    /// Begin (or re-pitch to) `note`.
    fn trigger(&mut self, note: u8, cfg: &Self::Cfg);
    /// Produce one raw mono sample and advance internal phase/position.
    fn render(&mut self, cfg: &Self::Cfg, dt: f32) -> f32;
    /// True once the source can produce no more sound (sampler buffer exhausted).
    fn finished(&self) -> bool;
    /// Hard-reset running state (phase / playback position).
    fn reset(&mut self, cfg: &Self::Cfg);
}

/// Shared, automatable voice parameters. `cutoff` is smoothed to avoid zipper
/// noise; the rest apply immediately (envelope times take effect on the next
/// trigger, matching musician expectations).
pub struct VoiceParams {
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
    pub cutoff: Smoothed,
    pub resonance: f32,
    pub filter_type: FilterType,
    pub lfo_rate: f32,
    pub lfo_depth: f32,
    pub retrigger: RetriggerMode,
}

struct Voice<S: Source> {
    source: S,
    env: Adsr,
    filter: Svf,
    note: u8,
    vel: f32,
    active: bool,
    last_used: u64,
}

impl<S: Source> Voice<S> {
    fn new(sample_rate: f32, cfg: &S::Cfg) -> Self {
        Self {
            source: S::new(sample_rate, cfg),
            env: Adsr::new(),
            filter: Svf::new(),
            note: 0,
            vel: 0.0,
            active: false,
            last_used: 0,
        }
    }
}

/// A polyphonic bank of voices sharing one `Source::Cfg` and one `VoiceParams`.
/// Generic over the sound source, so synth and sampler get identical allocation,
/// stealing, envelope, filter and LFO behaviour for free.
pub struct VoiceBank<S: Source> {
    sample_rate: f32,
    cfg: S::Cfg,
    voices: Vec<Voice<S>>,
    pub params: VoiceParams,
    lfo_phase: f32,
    clock: u64,
}

impl<S: Source> VoiceBank<S> {
    pub fn new(
        sample_rate: f32,
        polyphony: usize,
        cfg: S::Cfg,
        params: VoiceParams,
    ) -> Self {
        let voices = (0..polyphony.max(1))
            .map(|_| Voice::new(sample_rate, &cfg))
            .collect();
        Self {
            sample_rate,
            cfg,
            voices,
            params,
            lfo_phase: 0.0,
            clock: 0,
        }
    }

    /// Replace the shared source config (osc list / sample source) after a patch
    /// edit, keeping running voices alive.
    pub fn set_cfg(&mut self, cfg: S::Cfg) {
        self.cfg = cfg;
    }

    pub fn cfg(&self) -> &S::Cfg {
        &self.cfg
    }

    pub fn cfg_mut(&mut self) -> &mut S::Cfg {
        &mut self.cfg
    }

    pub fn note_on(&mut self, note: u8, vel: f32) {
        self.clock = self.clock.wrapping_add(1);
        let vel = vel.clamp(0.0, 1.0);

        let idx = self
            .voices
            .iter()
            .position(|v| v.active && v.note == note)
            .unwrap_or_else(|| {
                self.voices
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, v)| if v.active { v.last_used } else { 0 })
                    .map(|(i, _)| i)
                    .unwrap_or(0)
            });

        let p = &self.params;
        let (attack, decay, sustain, release, retrigger) =
            (p.attack, p.decay, p.sustain, p.release, p.retrigger);
        let cfg = &self.cfg;
        let v = &mut self.voices[idx];
        let was_active = v.active;

        v.active = true;
        v.note = note;
        v.vel = vel;
        v.last_used = self.clock;
        v.env.attack = attack;
        v.env.decay = decay;
        v.env.sustain = sustain;
        v.env.release = release;
        v.source.trigger(note, cfg);

        if was_active {
            // Retrigger an already-sounding voice per the retrigger mode.
            let hard_reset = v.env.retrigger(retrigger);
            if hard_reset {
                v.source.reset(cfg);
                v.filter.reset();
            }
        } else {
            v.source.reset(cfg);
            v.filter.reset();
            v.env.note_on();
        }
    }

    pub fn note_off(&mut self, note: u8) {
        for v in &mut self.voices {
            if v.active && v.note == note {
                v.env.note_off();
            }
        }
    }

    /// Release every sounding voice — used only when a pattern is *replaced*.
    pub fn all_notes_off(&mut self) {
        for v in &mut self.voices {
            if v.active {
                v.env.note_off();
            }
        }
    }

    /// Render `out.len()` mono samples, overwriting `out`.
    pub fn render_block(&mut self, out: &mut [f32]) {
        let dt = 1.0 / self.sample_rate;
        let sr = self.sample_rate;
        for slot in out.iter_mut() {
            let lfo = (std::f32::consts::TAU * self.lfo_phase).sin() * self.params.lfo_depth;
            self.lfo_phase = (self.lfo_phase + self.params.lfo_rate * dt).rem_euclid(1.0);
            let base_cutoff = self.params.cutoff.next();
            let cutoff = (base_cutoff * (1.0 + lfo)).clamp(20.0, 20000.0);

            let mut mix = 0.0;
            for v in &mut self.voices {
                if !v.env.is_active() {
                    v.active = false;
                    continue;
                }
                let raw = v.source.render(&self.cfg, dt);
                if v.source.finished() {
                    v.env.note_off();
                }
                let env = v.env.next_sample(dt);
                let filtered = v.filter.process(
                    raw * env * v.vel,
                    cutoff,
                    self.params.resonance,
                    self.params.filter_type,
                    sr,
                );
                mix += filtered;
            }
            *slot = mix;
        }
    }
}
