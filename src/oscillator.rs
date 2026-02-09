use std::f32::consts::PI;

use crate::waveform::Waveform;

#[derive(Clone, Debug, PartialEq)]
pub struct Oscillator {
    pub waveform: Waveform,
    pub detune_semitones: f32,
    pub level: f32,
    pub phase_offset: f32,
}

impl Oscillator {
    pub fn new(waveform: Waveform) -> Self {
        Self {
            waveform,
            detune_semitones: 0.0,
            level: 1.0,
            phase_offset: 0.0,
        }
    }

    pub fn waveform(mut self, w: Waveform) -> Self {
        self.waveform = w;
        self
    }

    pub fn detune(mut self, semitones: f32) -> Self {
        self.detune_semitones = semitones;
        self
    }

    pub fn level(mut self, level: f32) -> Self {
        self.level = level;
        self
    }

    pub fn phase_offset(mut self, offset: f32) -> Self {
        self.phase_offset = offset;
        self
    }

    // &mut self setters for scene API chaining (osc() returns &mut Oscillator)

    pub fn set_detune(&mut self, semitones: f32) -> &mut Self {
        self.detune_semitones = semitones;
        self
    }

    pub fn set_level(&mut self, level: f32) -> &mut Self {
        self.level = level;
        self
    }

    pub fn set_phase_offset(&mut self, offset: f32) -> &mut Self {
        self.phase_offset = offset;
        self
    }
}

#[derive(Clone, Debug)]
pub(crate) struct OscillatorState {
    configs: Vec<Oscillator>,
    phases: Vec<f32>,
}

impl OscillatorState {
    pub(crate) fn new(configs: Vec<Oscillator>) -> Self {
        let phases: Vec<f32> = configs.iter().map(|c| c.phase_offset).collect();
        Self { configs, phases }
    }

    pub(crate) fn render(&mut self, base_freq: f32, dt: f32) -> f32 {
        let mut total = 0.0;
        let mut total_level = 0.0;

        for (i, config) in self.configs.iter().enumerate() {
            let detune_ratio = 2.0_f32.powf(config.detune_semitones / 12.0);
            let freq = base_freq * detune_ratio;
            let sample = render_waveform(config.waveform, self.phases[i]);
            total += sample * config.level;
            total_level += config.level;

            self.phases[i] = (self.phases[i] + freq * dt) % 1.0;
        }

        if total_level > 0.0 {
            total / total_level
        } else {
            0.0
        }
    }

    pub(crate) fn reset(&mut self) {
        for (i, config) in self.configs.iter().enumerate() {
            self.phases[i] = config.phase_offset;
        }
    }

    pub(crate) fn set_configs(&mut self, configs: Vec<Oscillator>) {
        self.phases = configs.iter().map(|c| c.phase_offset).collect();
        self.configs = configs;
    }
}

fn render_waveform(wave: Waveform, phase: f32) -> f32 {
    match wave {
        Waveform::Sine => (2.0 * PI * phase).sin(),
        Waveform::Saw => 2.0 * (phase - 0.5),
        Waveform::Square => {
            if phase < 0.5 { 1.0 } else { -1.0 }
        }
        Waveform::Triangle => 4.0 * (phase - 0.5).abs() - 1.0,
    }
}
