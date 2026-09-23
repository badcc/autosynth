use std::f32::consts::PI;

/// Filter response. The first four share one state-variable filter; `Ladder`
/// is a 4-pole resonant lowpass with a saturating input — the acid filter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FilterType {
    Lowpass,
    Highpass,
    Bandpass,
    Notch,
    Ladder,
}

/// State-variable filter (Cytomic / Andrew Simper). Linear, numerically
/// stable, and modulation-friendly.
#[derive(Clone, Copy, Debug, Default)]
pub struct Svf {
    ic1eq: f32,
    ic2eq: f32,
}

impl Svf {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        self.ic1eq = 0.0;
        self.ic2eq = 0.0;
    }

    /// Process one sample. `resonance` is `0.0..1.0` (higher = more resonant).
    pub fn process(&mut self, sample: f32, cutoff: f32, resonance: f32, filter_type: FilterType, sample_rate: f32) -> f32 {
        let cutoff = cutoff.clamp(20.0, sample_rate * 0.49);
        let k = 2.0 - 2.0 * resonance.clamp(0.0, 0.99);

        let g = (PI * cutoff / sample_rate).tan();
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;

        let v3 = sample - self.ic2eq;
        let v1 = a1 * self.ic1eq + a2 * v3;
        let v2 = self.ic2eq + a2 * self.ic1eq + a3 * v3;

        self.ic1eq = 2.0 * v1 - self.ic1eq;
        self.ic2eq = 2.0 * v2 - self.ic2eq;

        match filter_type {
            FilterType::Lowpass | FilterType::Ladder => v2,
            FilterType::Highpass => sample - k * v1 - v2,
            FilterType::Bandpass => v1,
            FilterType::Notch => sample - k * v1,
        }
    }
}

/// Zero-delay-feedback 4-pole ladder (Zavalishin's TPT form). The feedback
/// loop is solved exactly for the linear part; a `tanh` on the loop input adds
/// the drive and keeps high resonance from blowing up. `resonance` 1.0
/// self-oscillates.
#[derive(Clone, Copy, Debug, Default)]
pub struct Ladder {
    s: [f32; 4],
}

impl Ladder {
    pub fn reset(&mut self) {
        self.s = [0.0; 4];
    }

    pub fn process(&mut self, x: f32, cutoff: f32, resonance: f32, sample_rate: f32) -> f32 {
        let cutoff = cutoff.clamp(20.0, sample_rate * 0.45);
        let g = (PI * cutoff / sample_rate).tan();
        let big_g = g / (1.0 + g);
        let beta = 1.0 / (1.0 + g);
        let k = 4.0 * resonance.clamp(0.0, 1.0);

        let g2 = big_g * big_g;
        let g3 = g2 * big_g;
        let g4 = g3 * big_g;
        let sigma = g3 * beta * self.s[0] + g2 * beta * self.s[1] + big_g * beta * self.s[2] + beta * self.s[3];
        // Passband gain compensation so resonance doesn't thin the sound out.
        let input = x * (1.0 + 0.5 * k);
        let y4_est = (g4 * input + sigma) / (1.0 + k * g4);
        let mut u = (input - k * y4_est).tanh();
        for s in &mut self.s {
            let v = (u - *s) * big_g;
            let y = v + *s;
            *s = y + v;
            u = y;
        }
        u
    }
}

/// One filter of any [`FilterType`], owning both topologies' state so the type
/// can change live without clicks from reallocation.
#[derive(Clone, Copy, Debug, Default)]
pub struct Filter {
    svf: Svf,
    ladder: Ladder,
}

impl Filter {
    pub fn reset(&mut self) {
        self.svf.reset();
        self.ladder.reset();
    }

    #[inline]
    pub fn process(&mut self, x: f32, kind: FilterType, cutoff: f32, resonance: f32, sample_rate: f32) -> f32 {
        match kind {
            FilterType::Ladder => self.ladder.process(x, cutoff, resonance, sample_rate),
            t => self.svf.process(x, cutoff, resonance, t, sample_rate),
        }
    }
}
