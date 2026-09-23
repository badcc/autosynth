use std::f32::consts::TAU;

use crate::dsp::waveform::Waveform;

/// One oscillator's structure: waveform, unison stack and start phase. Level
/// and pitch offset are modulated per voice, so they arrive at render time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Osc {
    pub wave: Waveform,
    /// Stacked copies (1 = no unison).
    pub unison: u8,
    /// Total detune width of the stack, in semitones.
    pub spread: f32,
    /// Start phase, `0..1`.
    pub phase: f32,
}

impl Osc {
    pub fn new(wave: Waveform) -> Self {
        Self { wave, unison: 1, spread: 0.0, phase: 0.0 }
    }

    fn voices(&self) -> usize {
        self.unison.max(1) as usize
    }
}

/// Per-voice oscillator state: one running phase per unison copy of every
/// oscillator, plus their per-sample increments (recomputed by `tune`).
#[derive(Clone, Debug)]
pub struct OscState {
    phases: Vec<f32>,
    incs: Vec<f32>,
    rng: u32,
}

impl Default for OscState {
    fn default() -> Self {
        Self::new()
    }
}

impl OscState {
    pub fn new() -> Self {
        Self { phases: Vec::new(), incs: Vec::new(), rng: 0x2545_f491 }
    }

    /// Reset every phase to its oscillator's start phase. Unison copies are
    /// spread by the golden ratio so the stack doesn't phase-cancel at onset.
    pub fn reset(&mut self, oscs: &[Osc]) {
        self.phases.clear();
        for o in oscs {
            for j in 0..o.voices() {
                self.phases.push((o.phase + j as f32 * 0.618_034).rem_euclid(1.0));
            }
        }
        self.incs.resize(self.phases.len(), 0.0);
    }

    /// Set pitch: `freq` is the voice frequency in Hz, `semis[i]` oscillator
    /// i's offset, `dt` one sample period. Call at modulation rate.
    pub fn tune(&mut self, oscs: &[Osc], freq: f32, semis: &[f32], dt: f32) {
        let total: usize = oscs.iter().map(Osc::voices).sum();
        if total != self.phases.len() {
            self.reset(oscs);
        }
        let mut k = 0;
        for (i, o) in oscs.iter().enumerate() {
            let n = o.voices();
            let base = semis.get(i).copied().unwrap_or(0.0);
            for j in 0..n {
                let detune = if n > 1 { o.spread * (j as f32 / (n - 1) as f32 - 0.5) } else { 0.0 };
                self.incs[k] = if o.wave == Waveform::Noise {
                    0.0
                } else {
                    freq * 2.0_f32.powf((base + detune) / 12.0) * dt
                };
                k += 1;
            }
        }
    }

    /// Render one sample: every oscillator's unison stack, scaled by `levels`.
    #[inline]
    pub fn render(&mut self, oscs: &[Osc], levels: &[f32]) -> f32 {
        let mut total = 0.0;
        let mut k = 0;
        for (i, o) in oscs.iter().enumerate() {
            let n = o.voices();
            if k + n > self.phases.len() {
                break;
            }
            let norm = levels.get(i).copied().unwrap_or(0.0) / (n as f32).sqrt();
            let mut stack = 0.0;
            for _ in 0..n {
                let (phase, inc) = (self.phases[k], self.incs[k]);
                stack += match o.wave {
                    Waveform::Sine => (TAU * phase).sin(),
                    Waveform::Triangle => 4.0 * (phase - 0.5).abs() - 1.0,
                    Waveform::Saw => 2.0 * phase - 1.0 - poly_blep(phase, inc),
                    Waveform::Square => {
                        let s = if phase < 0.5 { 1.0 } else { -1.0 };
                        s + poly_blep(phase, inc) - poly_blep((phase + 0.5).rem_euclid(1.0), inc)
                    }
                    Waveform::Noise => self.next_noise(),
                };
                self.phases[k] = (phase + inc).rem_euclid(1.0);
                k += 1;
            }
            total += stack * norm;
        }
        total
    }

    fn next_noise(&mut self) -> f32 {
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
