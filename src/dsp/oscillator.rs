use std::f32::consts::TAU;

use crate::dsp::waveform::Waveform;

/// One oscillator's static configuration within a patch.
#[derive(Clone, Copy, Debug, PartialEq)]
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
}

/// Per-voice oscillator phases. The configs live in the patch (shared, read at
/// render time); only the running phase is owned here.
#[derive(Clone, Debug, Default)]
pub struct OscPhases {
    phases: Vec<f32>,
    /// White-noise state (xorshift), independent of phase.
    rng: u32,
}

impl OscPhases {
    pub fn new(configs: &[Oscillator]) -> Self {
        Self {
            phases: configs.iter().map(|c| c.phase_offset).collect(),
            rng: 0x2545_f491,
        }
    }

    /// Reset all phases to their configured offsets.
    pub fn reset(&mut self, configs: &[Oscillator]) {
        self.phases.clear();
        self.phases.extend(configs.iter().map(|c| c.phase_offset));
    }

    /// Ensure the phase vector matches the config count (after a patch change).
    pub fn resize_to(&mut self, configs: &[Oscillator]) {
        if self.phases.len() != configs.len() {
            self.reset(configs);
        }
    }

    /// Render one summed sample across all oscillators at `base_freq`.
    /// `dt` is one sample period in seconds.
    pub fn render(&mut self, configs: &[Oscillator], base_freq: f32, dt: f32) -> f32 {
        self.resize_to(configs);
        let mut total = 0.0;
        for (i, config) in configs.iter().enumerate() {
            let inc = if config.waveform == Waveform::Noise {
                0.0
            } else {
                let ratio = 2.0_f32.powf(config.detune_semitones / 12.0);
                base_freq * ratio * dt
            };
            let phase = self.phases[i];
            let sample = match config.waveform {
                Waveform::Sine => (TAU * phase).sin(),
                Waveform::Triangle => 4.0 * (phase - 0.5).abs() - 1.0,
                Waveform::Saw => {
                    // Naive saw minus the polyBLEP correction at the wrap.
                    let mut s = 2.0 * phase - 1.0;
                    s -= poly_blep(phase, inc);
                    s
                }
                Waveform::Square => {
                    let mut s = if phase < 0.5 { 1.0 } else { -1.0 };
                    s += poly_blep(phase, inc);
                    s -= poly_blep((phase + 0.5).rem_euclid(1.0), inc);
                    s
                }
                Waveform::Noise => self.next_noise(),
            };
            total += sample * config.level;
            self.phases[i] = (phase + inc).rem_euclid(1.0);
        }
        total
    }

    fn next_noise(&mut self) -> f32 {
        // xorshift32 → uniform in -1..1
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

/// polyBLEP band-limiting correction for a discontinuity at phase 0/1.
/// `t` is the current phase, `dt` the per-sample phase increment.
fn poly_blep(t: f32, dt: f32) -> f32 {
    if dt <= 0.0 {
        return 0.0;
    }
    if t < dt {
        let x = t / dt;
        x + x - x * x - 1.0
    } else if t > 1.0 - dt {
        let x = (t - 1.0) / dt;
        x * x + x + x + 1.0
    } else {
        0.0
    }
}
