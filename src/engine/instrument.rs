//! The voice engine. One `Voice` type serves synth and sampler: a source
//! (unison oscillators or a sample playhead) → optional filter → amp
//! envelope × gain, with every parameter driven by a compiled [`Program`]
//! evaluated per voice every [`MOD_BLOCK`] samples.

use std::collections::HashMap;
use std::sync::Arc;

use crate::dsp::envelope::{Adsr, RetriggerMode};
use crate::dsp::filter::{Filter, FilterType};
use crate::dsp::midi_to_freq;
use crate::dsp::oscillator::{Osc, OscState};
use crate::model::instrument::{InstrumentSpec, SourceSpec};
use crate::music::phrase::Locks;
use crate::music::signal::{EnvShape, EvalCtx, MAX_ENVS, MAX_LFOS, Mod, Program, VoiceInputs, VoiceLayout};
use crate::sample::SampleData;

/// Samples between modulation evaluations (~0.36 ms at 44.1 kHz). Cutoff and
/// gain ramp linearly across the block.
pub const MOD_BLOCK: usize = 16;

/// Glide time used when a note slides on an instrument with no glide set.
const DEFAULT_SLIDE: f32 = 0.06;

/// Where a sampler voice reads audio from. Loaded on the control thread.
pub enum SampleSource {
    Pitched { data: Arc<SampleData>, root: u8 },
    Kit { map: HashMap<u8, Arc<SampleData>> },
}

pub enum SourceCfg {
    Osc(Vec<Osc>),
    Sample(SampleSource),
}

impl SourceCfg {
    fn same_kind(&self, other: &SourceCfg) -> bool {
        matches!((self, other), (SourceCfg::Osc(_), SourceCfg::Osc(_)) | (SourceCfg::Sample(_), SourceCfg::Sample(_)))
    }
}

/// Everything a voice reads, compiled from an [`InstrumentSpec`] on the
/// control thread.
pub struct InstrumentCfg {
    pub source: SourceCfg,
    pub filter: Option<FilterType>,
    pub amp: EnvShape,
    pub layout: VoiceLayout,
    pub pitch: Program,
    pub gain: Program,
    pub cutoff: Program,
    pub res: Program,
    pub osc_levels: Vec<Program>,
    pub osc_semis: Vec<Program>,
    pub voices: usize,
    pub mono: bool,
    pub glide: f32,
    pub retrigger: RetriggerMode,
    pub oneshot: bool,
}

impl InstrumentCfg {
    /// Compile `spec` against an already-resolved `source` (samples loaded).
    pub fn compile(spec: &InstrumentSpec, source: SourceCfg) -> Self {
        let mut layout = VoiceLayout::default();
        let mut voice = |m: &Mod| Program::voice(m, &mut layout);
        let (osc_levels, osc_semis) = match &spec.source {
            SourceSpec::Osc(oscs) => (oscs.iter().map(|o| voice(&o.level)).collect(), oscs.iter().map(|o| voice(&o.semis)).collect()),
            _ => (Vec::new(), Vec::new()),
        };
        let (cutoff, res) = match &spec.filter {
            Some(f) => (voice(&f.cutoff), voice(&f.res)),
            None => (Program::constant(20_000.0), Program::constant(0.0)),
        };
        let pitch = voice(&spec.pitch);
        let gain = voice(&spec.gain);
        InstrumentCfg {
            source,
            filter: spec.filter.as_ref().map(|f| f.kind),
            amp: spec.amp,
            layout,
            pitch,
            gain,
            cutoff,
            res,
            osc_levels,
            osc_semis,
            voices: if spec.mono { 1 } else { spec.voices.max(1) },
            mono: spec.mono,
            glide: spec.glide,
            retrigger: spec.retrigger,
            oneshot: spec.oneshot,
        }
    }

    /// The oscillator structure of a spec (for `SourceCfg::Osc`). An empty
    /// oscillator list plays a single saw.
    pub fn oscs(spec: &InstrumentSpec) -> Vec<Osc> {
        match &spec.source {
            SourceSpec::Osc(oscs) if !oscs.is_empty() => {
                oscs.iter().map(|o| Osc { wave: o.wave, unison: o.unison, spread: o.spread, phase: o.phase }).collect()
            }
            _ => vec![Osc::new(crate::dsp::waveform::Waveform::Saw)],
        }
    }
}

/// Modulation context for one render call: song beat at the first sample.
pub struct ModCtx<'a> {
    pub beat: f64,
    pub bps: f64,
    pub knobs: &'a [f32; 128],
}

fn apply_shape(env: &mut Adsr, s: &EnvShape) {
    env.attack = s.attack;
    env.decay = s.decay;
    env.sustain = s.sustain;
    env.release = s.release;
}

/// A sample playhead: fractional position and rate.
#[derive(Default)]
struct Playhead {
    data: Option<Arc<SampleData>>,
    pos: f64,
    rate: f64,
    /// File rate / engine rate.
    native: f64,
    /// The note that plays at native speed.
    root: f32,
    done: bool,
}

impl Playhead {
    fn trigger(&mut self, note: u8, src: &SampleSource, engine_sr: f32) {
        let (data, root) = match src {
            SampleSource::Pitched { data, root } => (Some(data), *root as f32),
            SampleSource::Kit { map } => (map.get(&note), note as f32),
        };
        self.data = data.cloned();
        self.root = root;
        self.native = self.data.as_ref().map_or(1.0, |d| d.sample_rate as f64 / engine_sr as f64);
        self.pos = 0.0;
        self.done = self.data.is_none();
    }

    fn set_pitch(&mut self, pitch: f32) {
        self.rate = self.native * 2.0_f64.powf((pitch - self.root) as f64 / 12.0);
    }

    #[inline]
    fn render(&mut self) -> f32 {
        let Some(data) = &self.data else {
            return 0.0;
        };
        let s = &data.samples;
        let i = self.pos as usize;
        if self.done || i + 1 >= s.len() {
            self.done = true;
            return 0.0;
        }
        let frac = (self.pos - i as f64) as f32;
        self.pos += self.rate;
        s[i] * (1.0 - frac) + s[i + 1] * frac
    }
}

struct Voice {
    osc: OscState,
    play: Playhead,
    amp: Adsr,
    envs: [Adsr; MAX_ENVS],
    lfo_phase: [f32; MAX_LFOS],
    filter: Filter,
    note: u8,
    vel: f32,
    rnd: f32,
    active: bool,
    /// Key held (between note-on and note-off).
    gate: bool,
    slide: bool,
    locks: Locks,
    last_used: u64,
    pitch: f32,
    target: f32,
    glide: f32,
    countdown: usize,
    fresh: bool,
    cutoff: f32,
    cutoff_step: f32,
    gain: f32,
    gain_step: f32,
    res: f32,
    levels: Vec<f32>,
    semis: Vec<f32>,
}

impl Voice {
    fn new(oscs: usize) -> Self {
        Voice {
            osc: OscState::new(),
            play: Playhead::default(),
            amp: Adsr::new(),
            envs: [Adsr::new(); MAX_ENVS],
            lfo_phase: [0.0; MAX_LFOS],
            filter: Filter::default(),
            note: 60,
            vel: 0.0,
            rnd: 0.0,
            active: false,
            gate: false,
            slide: false,
            locks: Locks::default(),
            last_used: 0,
            pitch: 60.0,
            target: 60.0,
            glide: 0.0,
            countdown: 0,
            fresh: true,
            cutoff: 20_000.0,
            cutoff_step: 0.0,
            gain: 0.0,
            gain_step: 0.0,
            res: 0.0,
            levels: vec![0.0; oscs],
            semis: vec![0.0; oscs],
        }
    }

    fn release(&mut self) {
        self.gate = false;
        self.amp.note_off();
        for e in &mut self.envs {
            e.note_off();
        }
    }

    /// Evaluate every modulation program and set up this block's ramps.
    fn modulate(&mut self, cfg: &InstrumentCfg, beat: f64, knobs: &[f32; 128], dt: f32, sr: f32) {
        let block_dt = dt * MOD_BLOCK as f32;
        let mut inputs = VoiceInputs { vel: self.vel, key: (self.note as f32 - 60.0) / 12.0, rnd: self.rnd, ..Default::default() };
        for (i, _) in cfg.layout.envs.iter().enumerate() {
            inputs.env[i] = self.envs[i].level;
        }
        for (i, (hz, wave)) in cfg.layout.lfos.iter().enumerate() {
            inputs.lfo[i] = wave.at(self.lfo_phase[i] as f64);
            self.lfo_phase[i] = (self.lfo_phase[i] + hz * block_dt).rem_euclid(1.0);
        }
        let ctx = EvalCtx { beat, knobs, voice: &inputs };

        if self.pitch != self.target {
            if self.glide > 0.0 {
                self.pitch += (self.target - self.pitch) * (1.0 - (-block_dt / self.glide).exp());
                if (self.target - self.pitch).abs() < 1e-3 {
                    self.pitch = self.target;
                }
            } else {
                self.pitch = self.target;
            }
        }
        let pitch = self.pitch + cfg.pitch.eval(&ctx);
        match &cfg.source {
            SourceCfg::Osc(oscs) => {
                self.levels.resize(oscs.len(), 0.0);
                self.semis.resize(oscs.len(), 0.0);
                for i in 0..oscs.len() {
                    self.levels[i] = cfg.osc_levels.get(i).map_or(1.0, |p| p.eval(&ctx));
                    self.semis[i] = cfg.osc_semis.get(i).map_or(0.0, |p| p.eval(&ctx));
                }
                self.osc.tune(oscs, midi_to_freq(pitch), &self.semis, dt);
            }
            SourceCfg::Sample(_) => self.play.set_pitch(pitch),
        }

        let cutoff = self.locks.cutoff.unwrap_or_else(|| cfg.cutoff.eval(&ctx)).clamp(20.0, sr * 0.45);
        let gain = self.locks.gain.unwrap_or_else(|| cfg.gain.eval(&ctx)).max(0.0);
        self.res = self.locks.res.unwrap_or_else(|| cfg.res.eval(&ctx)).clamp(0.0, 1.0);
        if self.fresh {
            self.fresh = false;
            (self.cutoff, self.gain) = (cutoff, gain);
            (self.cutoff_step, self.gain_step) = (0.0, 0.0);
        } else {
            self.cutoff_step = (cutoff - self.cutoff) / MOD_BLOCK as f32;
            self.gain_step = (gain - self.gain) / MOD_BLOCK as f32;
        }
    }
}

/// A polyphonic (or mono) instrument: a voice pool sharing one config.
pub struct Instrument {
    cfg: InstrumentCfg,
    voices: Vec<Voice>,
    clock: u64,
    rng: u32,
    sample_rate: f32,
}

impl Instrument {
    pub fn new(cfg: InstrumentCfg, sample_rate: f32) -> Self {
        let oscs = match &cfg.source {
            SourceCfg::Osc(o) => o.len(),
            _ => 0,
        };
        let voices = (0..cfg.voices).map(|_| Voice::new(oscs)).collect();
        Self { cfg, voices, clock: 0, rng: 0x9E37_79B9, sample_rate }
    }

    /// Swap in an edited config. Running voices keep playing unless the voice
    /// count or source kind changed.
    pub fn set_cfg(&mut self, cfg: InstrumentCfg) {
        if cfg.voices != self.voices.len() || !cfg.source.same_kind(&self.cfg.source) {
            *self = Instrument::new(cfg, self.sample_rate);
        } else {
            self.cfg = cfg;
        }
    }

    fn next_rnd(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }

    pub fn note_on(&mut self, note: u8, vel: f32, slide: bool, locks: Locks) {
        self.clock = self.clock.wrapping_add(1);
        let vel = vel.clamp(0.0, 1.0);
        let rnd = self.next_rnd();
        let cfg = &self.cfg;

        let idx = if cfg.mono {
            let v = &mut self.voices[0];
            // Legato: a new note while the previous is held glides instead of
            // retriggering when it slides, the synth glides, or mode is Legato.
            if v.active && v.gate && (v.slide || cfg.glide > 0.0 || cfg.retrigger == RetriggerMode::Legato) {
                v.glide = if cfg.glide > 0.0 { cfg.glide } else if v.slide { DEFAULT_SLIDE } else { 0.0 };
                v.note = note;
                v.target = note as f32;
                v.vel = vel;
                v.slide = slide;
                v.locks = locks;
                v.last_used = self.clock;
                return;
            }
            0
        } else {
            self.voices.iter().position(|v| v.active && v.note == note).unwrap_or_else(|| {
                self.voices
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, v)| if v.active { v.last_used } else { 0 })
                    .map_or(0, |(i, _)| i)
            })
        };

        let sr = self.sample_rate;
        let v = &mut self.voices[idx];
        let was_active = v.active;
        v.active = true;
        v.gate = true;
        v.note = note;
        v.vel = vel;
        v.rnd = rnd;
        v.slide = slide;
        v.locks = locks;
        v.last_used = self.clock;
        v.target = note as f32;
        v.glide = cfg.glide;
        if !(cfg.mono && was_active && cfg.glide > 0.0) {
            v.pitch = note as f32;
        }

        apply_shape(&mut v.amp, &cfg.amp);
        if let Some(d) = locks.decay {
            v.amp.decay = d.max(0.0);
        }
        for (e, shape) in v.envs.iter_mut().zip(&cfg.layout.envs) {
            apply_shape(e, shape);
        }
        if let SourceCfg::Sample(src) = &cfg.source {
            v.play.trigger(note, src, sr);
        }

        if was_active {
            if v.amp.retrigger(cfg.retrigger) {
                if let SourceCfg::Osc(oscs) = &cfg.source {
                    v.osc.reset(oscs);
                }
                v.filter.reset();
            }
            let mode = if cfg.retrigger == RetriggerMode::Hard { RetriggerMode::Hard } else { RetriggerMode::Soft };
            for e in &mut v.envs {
                e.retrigger(mode);
            }
        } else {
            if let SourceCfg::Osc(oscs) = &cfg.source {
                v.osc.reset(oscs);
            }
            v.filter.reset();
            v.amp.note_on();
            for e in &mut v.envs {
                e.note_on();
            }
            v.lfo_phase = [0.0; MAX_LFOS];
            v.fresh = true;
        }
        v.countdown = 0;
    }

    pub fn note_off(&mut self, note: u8) {
        if self.cfg.oneshot {
            return;
        }
        for v in &mut self.voices {
            if v.active && v.gate && v.note == note {
                v.release();
            }
        }
    }

    pub fn all_notes_off(&mut self) {
        for v in &mut self.voices {
            if v.active {
                v.release();
            }
        }
    }

    /// Render `out.len()` mono samples, overwriting `out`.
    pub fn render(&mut self, out: &mut [f32], ctx: &ModCtx) {
        out.fill(0.0);
        let sr = self.sample_rate;
        let dt = 1.0 / sr;
        let cfg = &self.cfg;
        let n_envs = cfg.layout.envs.len();
        for v in &mut self.voices {
            if !v.active {
                continue;
            }
            for (i, slot) in out.iter_mut().enumerate() {
                if v.countdown == 0 {
                    v.modulate(cfg, ctx.beat + i as f64 * ctx.bps, ctx.knobs, dt, sr);
                    v.countdown = MOD_BLOCK;
                }
                v.countdown -= 1;
                for e in &mut v.envs[..n_envs] {
                    e.next_sample(dt);
                }
                let raw = match &cfg.source {
                    SourceCfg::Osc(oscs) => v.osc.render(oscs, &v.levels),
                    SourceCfg::Sample(_) => {
                        let s = v.play.render();
                        if v.play.done && v.gate {
                            v.release();
                        }
                        s
                    }
                };
                let amp = v.amp.next_sample(dt);
                if !v.amp.is_active() {
                    v.active = false;
                    break;
                }
                v.cutoff += v.cutoff_step;
                v.gain += v.gain_step;
                let y = match cfg.filter {
                    Some(kind) => v.filter.process(raw, kind, v.cutoff, v.res, sr),
                    None => raw,
                };
                *slot += y * amp * v.gain;
            }
        }
    }
}
