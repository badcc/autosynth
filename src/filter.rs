use std::f32::consts::PI;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FilterType {
    Lowpass,
    Highpass,
    Bandpass,
    Notch,
}

/// Per-voice state-variable filter (Cytomic/Andrew Simper SVF).
///
/// Linear, numerically stable, and supports modulation-friendly cutoff changes.
#[derive(Clone, Copy, Debug)]
pub struct Svf {
    ic1eq: f32,
    ic2eq: f32,
}

impl Default for Svf {
    fn default() -> Self {
        Self::new()
    }
}

impl Svf {
    pub fn new() -> Self {
        Self {
            ic1eq: 0.0,
            ic2eq: 0.0,
        }
    }

    pub fn reset(&mut self) {
        self.ic1eq = 0.0;
        self.ic2eq = 0.0;
    }

    /// Process a single sample through the SVF.
    ///
    /// `cutoff`: filter frequency in Hz (clamped to Nyquist)
    /// `resonance`: 0.0 (no resonance) to 1.0 (self-oscillation)
    /// `filter_type`: which output to return
    /// `sample_rate`: audio sample rate in Hz
    pub fn process(
        &mut self,
        sample: f32,
        cutoff: f32,
        resonance: f32,
        filter_type: FilterType,
        sample_rate: f32,
    ) -> f32 {
        // Clamp cutoff to avoid instability near Nyquist
        let cutoff = cutoff.clamp(20.0, sample_rate * 0.49);
        // Map resonance 0..1 to Q: Q = 0.5 (high res) to ~infinity (no res)
        // k = 1/Q, so k ranges from 2 (no resonance) to ~0 (self-oscillation)
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
            FilterType::Lowpass => v2,
            FilterType::Highpass => sample - k * v1 - v2,
            FilterType::Bandpass => v1,
            FilterType::Notch => sample - k * v1,
        }
    }
}
