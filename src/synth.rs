use std::f32::consts::PI;

use crate::automation::OscParam;
use crate::envelope::{Adsr, RetriggerMode};
use crate::event::{EventKind, SynthParam};
use crate::filter::{FilterType, Svf};
use crate::oscillator::OscillatorState;
use crate::patch::Patch;
use crate::waveform::Waveform;

#[derive(Clone, Debug)]
struct Voice {
    active: bool,
    note: u8,
    velocity: f32,
    oscillators: OscillatorState,
    filter: Svf,
    env: Adsr,
    last_used: u64,
}

impl Voice {
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
            oscillators: OscillatorState::new(patch.oscillators.clone()),
            filter: Svf::new(),
            env,
            last_used: 0,
        }
    }
}

pub struct Synth {
    sample_rate: f32,
    patch: Patch,
    voices: Vec<Voice>,
    lfo_phase: f32,
    clock: u64,
}

impl Synth {
    pub fn new(sample_rate: f32, polyphony: usize) -> Self {
        let patch = Patch::new();
        Self {
            sample_rate,
            voices: (0..polyphony).map(|_| Voice::new(&patch)).collect(),
            patch,
            lfo_phase: 0.0,
            clock: 0,
        }
    }

    pub fn with_patch(sample_rate: f32, polyphony: usize, patch: Patch) -> Self {
        Self {
            sample_rate,
            voices: (0..polyphony).map(|_| Voice::new(&patch)).collect(),
            patch,
            lfo_phase: 0.0,
            clock: 0,
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
        self.propagate_to_voices();
    }

    fn propagate_to_voices(&mut self) {
        for v in &mut self.voices {
            v.oscillators.set_configs(self.patch.oscillators.clone());
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

    pub(crate) fn set_osc_param(&mut self, index: usize, param: OscParam, value: f32) {
        if let Some(osc) = self.patch.oscillators.get_mut(index) {
            match param {
                OscParam::Detune => osc.detune_semitones = value,
                OscParam::Level => osc.level = value,
            }
        }
        for v in &mut self.voices {
            v.oscillators.set_param(index, param, value);
        }
    }

    pub fn set_osc_waveform(&mut self, index: usize, waveform: Waveform) {
        if let Some(osc) = self.patch.oscillators.get_mut(index) {
            osc.waveform = waveform;
        }
        for v in &mut self.voices {
            v.oscillators.set_waveform(index, waveform);
        }
    }

    pub fn note_on(&mut self, note: u8, vel: f32) {
        self.clock = self.clock.wrapping_add(1);
        let vel = vel.clamp(0.0, 1.0);

        // Same-note retrigger: reuse existing voice playing this note
        let idx = self
            .voices
            .iter()
            .position(|v| v.active && v.note == note)
            .unwrap_or_else(|| {
                // No existing voice for this note — steal the oldest inactive or LRU active
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

        if was_active {
            // Voice stealing or same-note retrigger — defer to RetriggerMode.
            // Hard resets oscillators/filter; Soft and Legato preserve phase continuity.
            let hard_reset = v.env.retrigger(self.patch.retrigger);
            if hard_reset {
                v.oscillators.set_configs(self.patch.oscillators.clone());
                v.oscillators.reset();
                v.filter.reset();
            }
            // Always sync envelope timing so the retrigger uses current ADSR values.
            v.env.attack = self.patch.attack;
            v.env.decay = self.patch.decay;
            v.env.sustain = self.patch.sustain;
            v.env.release = self.patch.release;
        } else {
            // Fresh voice — always start clean.
            v.oscillators.set_configs(self.patch.oscillators.clone());
            v.oscillators.reset();
            v.filter.reset();
            v.env.attack = self.patch.attack;
            v.env.decay = self.patch.decay;
            v.env.sustain = self.patch.sustain;
            v.env.release = self.patch.release;
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

            let freq = midi_to_freq(v.note as f32);
            let osc_sample = v.oscillators.render(freq, dt);
            let env = v.env.next_sample(dt);

            let voice = osc_sample * env * v.velocity;
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

fn midi_to_freq(note: f32) -> f32 {
    440.0 * 2.0_f32.powf((note - 69.0) / 12.0)
}
