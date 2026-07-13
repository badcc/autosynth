/// A one-pole parameter smoother. Automation writes a target once per control
/// period; the audible value glides toward it per sample, killing zipper noise
/// and clicks on cutoff/gain sweeps.
#[derive(Clone, Copy, Debug)]
pub struct Smoothed {
    value: f32,
    target: f32,
    /// Per-sample smoothing coefficient in `0.0..1.0`. Higher = snappier.
    coeff: f32,
}

impl Smoothed {
    /// `time_ms` is the ~63% convergence time.
    pub fn new(initial: f32, time_ms: f32, sample_rate: f32) -> Self {
        let mut s = Self {
            value: initial,
            target: initial,
            coeff: 1.0,
        };
        s.set_time(time_ms, sample_rate);
        s
    }

    pub fn set_time(&mut self, time_ms: f32, sample_rate: f32) {
        let samples = (time_ms * 0.001 * sample_rate).max(1.0);
        self.coeff = 1.0 - (-1.0 / samples).exp();
    }

    /// Set the target the value glides toward.
    pub fn set_target(&mut self, target: f32) {
        self.target = target;
    }

    /// Jump immediately to a value (no glide) — used on note retrigger / reset.
    pub fn snap(&mut self, value: f32) {
        self.value = value;
        self.target = value;
    }

    /// Advance one sample and return the smoothed value.
    #[inline]
    pub fn next(&mut self) -> f32 {
        self.value += (self.target - self.value) * self.coeff;
        self.value
    }

    pub fn value(&self) -> f32 {
        self.value
    }

    pub fn target(&self) -> f32 {
        self.target
    }
}
