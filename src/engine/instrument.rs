use std::collections::HashMap;
use std::sync::Arc;

use crate::dsp::midi_to_freq;
use crate::dsp::oscillator::{OscPhases, Oscillator};
use crate::dsp::smooth::Smoothed;
use crate::engine::voice::{Source, VoiceBank, VoiceParams};
use crate::model::PatchSpec;
use crate::sample::SampleData;

// ── Synth source ──

/// The synth voice source: per-voice oscillator phases. The osc configs live in
/// the shared `Cfg`, read at render time.
pub struct OscBank {
    phases: OscPhases,
    note: u8,
}

impl Source for OscBank {
    type Cfg = Vec<Oscillator>;

    fn new(_sample_rate: f32, cfg: &Self::Cfg) -> Self {
        Self {
            phases: OscPhases::new(cfg),
            note: 60,
        }
    }

    fn trigger(&mut self, note: u8, _cfg: &Self::Cfg) {
        self.note = note;
    }

    fn render(&mut self, cfg: &Self::Cfg, dt: f32) -> f32 {
        let freq = midi_to_freq(self.note as f32);
        self.phases.render(cfg, freq, dt)
    }

    fn finished(&self) -> bool {
        false
    }

    fn reset(&mut self, cfg: &Self::Cfg) {
        self.phases.reset(cfg);
    }
}

// ── Sampler source ──

/// Where a sampler voice reads audio from.
pub enum SampleSource {
    /// Pitched playback: `root` plays at native speed, other notes resample.
    Pitched { data: Arc<SampleData>, root: u8 },
    /// Drum kit: each MIDI note maps to its own sample, played at native pitch.
    Kit { map: HashMap<u8, Arc<SampleData>> },
}

/// The sampler voice source: a playhead into one sample buffer.
pub struct SamplePlayhead {
    engine_sr: f32,
    pos: f64,
    rate: f64,
    data: Option<Arc<SampleData>>,
    exhausted: bool,
}

impl Source for SamplePlayhead {
    type Cfg = SampleSource;

    fn new(sample_rate: f32, _cfg: &Self::Cfg) -> Self {
        Self {
            engine_sr: sample_rate,
            pos: 0.0,
            rate: 1.0,
            data: None,
            exhausted: true,
        }
    }

    fn trigger(&mut self, note: u8, cfg: &Self::Cfg) {
        let resolved = match cfg {
            SampleSource::Pitched { data, root } => {
                let semis = note as f64 - *root as f64;
                let sr_ratio = data.sample_rate as f64 / self.engine_sr as f64;
                Some((Arc::clone(data), 2.0_f64.powf(semis / 12.0) * sr_ratio))
            }
            SampleSource::Kit { map } => map.get(&note).map(|d| {
                let sr_ratio = d.sample_rate as f64 / self.engine_sr as f64;
                (Arc::clone(d), sr_ratio)
            }),
        };
        match resolved {
            Some((data, rate)) => {
                self.data = Some(data);
                self.rate = rate;
                self.pos = 0.0;
                self.exhausted = false;
            }
            None => {
                self.data = None;
                self.exhausted = true;
            }
        }
    }

    fn render(&mut self, _cfg: &Self::Cfg, _dt: f32) -> f32 {
        let Some(data) = &self.data else {
            return 0.0;
        };
        let samples = &data.samples;
        let len = samples.len();
        if self.exhausted || len == 0 {
            return 0.0;
        }
        let idx = self.pos as usize;
        let out = if idx >= len - 1 {
            self.exhausted = true;
            if idx < len { samples[idx] } else { 0.0 }
        } else {
            let frac = (self.pos - idx as f64) as f32;
            samples[idx] * (1.0 - frac) + samples[idx + 1] * frac
        };
        self.pos += self.rate;
        out
    }

    fn finished(&self) -> bool {
        self.exhausted
    }

    fn reset(&mut self, _cfg: &Self::Cfg) {
        self.pos = 0.0;
    }
}

// ── Instrument: one small enum over the two banks ──

/// A track's sound generator. Because everything shared lives in `VoiceBank`,
/// the enum only forwards note events, rendering, and cfg swaps.
pub enum Instrument {
    Synth(VoiceBank<OscBank>),
    Sampler(VoiceBank<SamplePlayhead>),
}

impl Instrument {
    pub fn synth(sample_rate: f32, polyphony: usize, patch: &PatchSpec) -> Self {
        Instrument::Synth(VoiceBank::new(
            sample_rate,
            polyphony,
            patch.oscillators.clone(),
            params_from_patch(patch, sample_rate),
        ))
    }

    pub fn sampler(
        sample_rate: f32,
        polyphony: usize,
        patch: &PatchSpec,
        source: SampleSource,
    ) -> Self {
        Instrument::Sampler(VoiceBank::new(
            sample_rate,
            polyphony,
            source,
            params_from_patch(patch, sample_rate),
        ))
    }

    pub fn params_mut(&mut self) -> &mut VoiceParams {
        match self {
            Instrument::Synth(b) => &mut b.params,
            Instrument::Sampler(b) => &mut b.params,
        }
    }

    pub fn note_on(&mut self, note: u8, vel: f32) {
        match self {
            Instrument::Synth(b) => b.note_on(note, vel),
            Instrument::Sampler(b) => b.note_on(note, vel),
        }
    }

    pub fn note_off(&mut self, note: u8) {
        match self {
            Instrument::Synth(b) => b.note_off(note),
            Instrument::Sampler(b) => b.note_off(note),
        }
    }

    pub fn all_notes_off(&mut self) {
        match self {
            Instrument::Synth(b) => b.all_notes_off(),
            Instrument::Sampler(b) => b.all_notes_off(),
        }
    }

    pub fn render_block(&mut self, out: &mut [f32]) {
        match self {
            Instrument::Synth(b) => b.render_block(out),
            Instrument::Sampler(b) => b.render_block(out),
        }
    }

    /// Apply oscillator changes from a hot-reloaded patch (synth only).
    pub fn set_oscillators(&mut self, oscs: Vec<Oscillator>) {
        if let Instrument::Synth(b) = self {
            b.set_cfg(oscs);
        }
    }

    /// Set one oscillator's level (synth only). Voices read the shared cfg, so a
    /// single write here reaches every voice — no per-voice copies.
    pub fn set_osc_level(&mut self, index: usize, level: f32) {
        if let Instrument::Synth(b) = self
            && let Some(o) = b.cfg_mut().get_mut(index)
        {
            o.level = level;
        }
    }

    pub fn set_osc_detune(&mut self, index: usize, semis: f32) {
        if let Instrument::Synth(b) = self
            && let Some(o) = b.cfg_mut().get_mut(index)
        {
            o.detune_semitones = semis;
        }
    }

    /// Apply a hot-reloaded patch's sound parameters, keeping running voices.
    pub fn apply_patch(&mut self, patch: &PatchSpec) {
        {
            let p = self.params_mut();
            p.attack = patch.attack;
            p.decay = patch.decay;
            p.sustain = patch.sustain;
            p.release = patch.release;
            p.cutoff.set_target(patch.cutoff);
            p.resonance = patch.resonance;
            p.filter_type = patch.filter_type;
            p.lfo_rate = patch.lfo_rate;
            p.lfo_depth = patch.lfo_depth;
            p.retrigger = patch.retrigger;
        }
        self.set_oscillators(patch.oscillators.clone());
    }
}

/// Build the shared voice params from a patch spec.
pub fn params_from_patch(patch: &PatchSpec, sample_rate: f32) -> VoiceParams {
    VoiceParams {
        attack: patch.attack,
        decay: patch.decay,
        sustain: patch.sustain,
        release: patch.release,
        cutoff: Smoothed::new(patch.cutoff, 8.0, sample_rate),
        resonance: patch.resonance,
        filter_type: patch.filter_type,
        lfo_rate: patch.lfo_rate,
        lfo_depth: patch.lfo_depth,
        retrigger: patch.retrigger,
    }
}
