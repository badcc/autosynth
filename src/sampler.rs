use std::collections::HashMap;
use std::f32::consts::PI;
use std::sync::Arc;

use crate::automation::OscParam;
use crate::envelope::{Adsr, RetriggerMode};
use crate::event::{EventKind, SynthParam};
use crate::filter::{FilterType, Svf};
use crate::patch::Patch;
use crate::sample::SampleData;
use crate::waveform::Waveform;

pub(crate) enum SamplerSource {
    Pitched { data: Arc<SampleData>, root_note: u8 },
    Kit { map: HashMap<u8, Arc<SampleData>> },
}

#[derive(Clone)]
struct SamplerVoice {
    active: bool,
    note: u8,
    velocity: f32,
    position: f64,
    rate: f64,
    filter: Svf,
    env: Adsr,
    last_used: u64,
    /// True when the sample buffer has been exhausted (one-shot end).
    exhausted: bool,
    /// Per-voice sample data (set on note_on from the source).
    data: Option<Arc<SampleData>>,
}

impl SamplerVoice {
    fn new(patch: &Patch) -> Self {
        let mut env = Adsr::new();
        env.attack = patch.attack;
        env.decay = patch.decay;
        env.sustain = patch.sustain;
        env.release = patch.release;
        Self {
            active: false,
            note: 0,
            velocity: 0.0,
            position: 0.0,
            rate: 1.0,
            filter: Svf::new(),
            env,
            last_used: 0,
            exhausted: false,
            data: None,
        }
    }
}

pub struct Sampler {
    sample_rate: f32,
    patch: Patch,
    voices: Vec<SamplerVoice>,
    lfo_phase: f32,
    clock: u64,
    source: SamplerSource,
}

impl Sampler {
    pub fn new(
        sample_rate: f32,
        polyphony: usize,
        patch: Patch,
        data: Arc<SampleData>,
        root_note: u8,
    ) -> Self {
        Self {
            sample_rate,
            voices: (0..polyphony).map(|_| SamplerVoice::new(&patch)).collect(),
            patch,
            lfo_phase: 0.0,
            clock: 0,
            source: SamplerSource::Pitched { data, root_note },
        }
    }

    pub fn new_kit(
        sample_rate: f32,
        polyphony: usize,
        patch: Patch,
        map: HashMap<u8, Arc<SampleData>>,
    ) -> Self {
        Self {
            sample_rate,
            voices: (0..polyphony).map(|_| SamplerVoice::new(&patch)).collect(),
            patch,
            lfo_phase: 0.0,
            clock: 0,
            source: SamplerSource::Kit { map },
        }
    }

    pub fn apply_event(&mut self, event: EventKind) {
        match event {
            EventKind::NoteOn { note, vel } => self.note_on(note, vel),
            EventKind::NoteOff { note } => self.note_off(note),
            EventKind::Param { param, value } => self.set_param(param, value),
            EventKind::SetPatch { patch } => self.apply_patch(patch),
        }
    }

    pub fn apply_patch(&mut self, patch: Patch) {
        self.patch = patch;
        for v in &mut self.voices {
            v.env.attack = self.patch.attack;
            v.env.decay = self.patch.decay;
            v.env.sustain = self.patch.sustain;
            v.env.release = self.patch.release;
        }
    }

    pub fn set_param(&mut self, param: SynthParam, val: f32) {
        match param {
            SynthParam::Attack => self.patch.attack = val.max(0.0),
            SynthParam::Decay => self.patch.decay = val.max(0.0),
            SynthParam::Sustain => self.patch.sustain = val.clamp(0.0, 1.0),
            SynthParam::Release => self.patch.release = val.max(0.0),
            SynthParam::Cutoff => self.patch.cutoff = val.clamp(20.0, 20000.0),
            SynthParam::Resonance => self.patch.resonance = val.clamp(0.0, 1.0),
            SynthParam::LfoRate => self.patch.lfo_rate = val.clamp(0.01, 20.0),
            SynthParam::LfoDepth => self.patch.lfo_depth = val.clamp(0.0, 1.0),
            SynthParam::MasterGain => self.patch.master_gain = val.clamp(0.0, 1.0),
        }
        if matches!(
            param,
            SynthParam::Attack | SynthParam::Decay | SynthParam::Sustain | SynthParam::Release
        ) {
            for v in &mut self.voices {
                v.env.attack = self.patch.attack;
                v.env.decay = self.patch.decay;
                v.env.sustain = self.patch.sustain;
                v.env.release = self.patch.release;
            }
        }
    }

    pub fn set_filter_type(&mut self, ft: FilterType) {
        self.patch.filter_type = ft;
    }

    pub fn set_retrigger(&mut self, mode: RetriggerMode) {
        self.patch.retrigger = mode;
    }

    pub(crate) fn set_osc_param(&mut self, _index: usize, _param: OscParam, _value: f32) {
        // No-op: sampler has no oscillators
    }

    pub fn set_osc_waveform(&mut self, _index: usize, _waveform: Waveform) {
        // No-op: sampler has no oscillators
    }

    pub fn note_on(&mut self, note: u8, vel: f32) {
        // Resolve sample data and playback rate from the source
        let (data, rate) = match &self.source {
            SamplerSource::Pitched { data, root_note } => {
                let semitone_diff = note as f64 - *root_note as f64;
                let sr_ratio = data.sample_rate as f64 / self.sample_rate as f64;
                let rate = 2.0_f64.powf(semitone_diff / 12.0) * sr_ratio;
                (Arc::clone(data), rate)
            }
            SamplerSource::Kit { map } => {
                let Some(data) = map.get(&note) else {
                    return; // Note not in kit — silently ignore
                };
                let sr_ratio = data.sample_rate as f64 / self.sample_rate as f64;
                (Arc::clone(data), sr_ratio)
            }
        };

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
                    .unwrap()
                    .0
            });

        let v = &mut self.voices[idx];
        let was_active = v.active;
        v.active = true;
        v.note = note;
        v.velocity = vel;
        v.last_used = self.clock;
        v.exhausted = false;
        v.data = Some(data);
        v.rate = rate;

        // Sampler always restarts from position 0 — unlike a synth oscillator,
        // a sample that's already played through must restart on retrigger.
        v.position = 0.0;
        v.filter.reset();
        v.env.attack = self.patch.attack;
        v.env.decay = self.patch.decay;
        v.env.sustain = self.patch.sustain;
        v.env.release = self.patch.release;

        if was_active {
            v.env.retrigger(self.patch.retrigger);
        } else {
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

    pub fn all_notes_off(&mut self) {
        for v in &mut self.voices {
            if v.active {
                v.env.note_off();
            }
        }
    }

    pub fn render_sample(&mut self) -> f32 {
        let dt = 1.0 / self.sample_rate;

        let lfo = (2.0 * PI * self.lfo_phase).sin() * self.patch.lfo_depth;
        self.lfo_phase = (self.lfo_phase + self.patch.lfo_rate * dt) % 1.0;

        let cutoff = (self.patch.cutoff * (1.0 + lfo)).clamp(20.0, 20000.0);

        let mut sample = 0.0;

        for v in &mut self.voices {
            if !v.env.is_active() {
                v.active = false;
                continue;
            }

            let samples = match &v.data {
                Some(d) => &d.samples,
                None => continue,
            };
            let len = samples.len();

            // Read sample with linear interpolation
            let raw = if v.exhausted || len == 0 {
                0.0
            } else {
                let pos = v.position;
                let idx = pos as usize;
                if idx >= len - 1 {
                    // Sample buffer exhausted — trigger release
                    v.exhausted = true;
                    v.env.note_off();
                    if idx < len { samples[idx] } else { 0.0 }
                } else {
                    let frac = (pos - idx as f64) as f32;
                    samples[idx] * (1.0 - frac) + samples[idx + 1] * frac
                }
            };

            // Advance position
            v.position += v.rate;

            let env = v.env.next_sample(dt);
            let voice = raw * env * v.velocity;
            let filtered = v.filter.process(
                voice,
                cutoff,
                self.patch.resonance,
                self.patch.filter_type,
                self.sample_rate,
            );
            sample += filtered;
        }

        (sample * self.patch.master_gain).clamp(-1.0, 1.0)
    }
}
