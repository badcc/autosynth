use crate::filter::FilterType;
use crate::oscillator::Oscillator;
use crate::waveform::Waveform;

#[derive(Clone, Debug)]
pub struct Patch {
    pub oscillators: Vec<Oscillator>,
    pub cutoff: f32,
    pub resonance: f32,
    pub filter_type: FilterType,
    pub lfo_rate: f32,
    pub lfo_depth: f32,
    pub master_gain: f32,
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
}

impl Default for Patch {
    fn default() -> Self {
        Self::new()
    }
}

impl Patch {
    pub fn new() -> Self {
        Self {
            oscillators: vec![
                Oscillator::new(Waveform::Saw).level(0.5),
                Oscillator::new(Waveform::Square)
                    .detune(0.1)
                    .level(0.5)
                    .phase_offset(0.25),
            ],
            cutoff: 8000.0,
            resonance: 0.0,
            filter_type: FilterType::Lowpass,
            lfo_rate: 0.5,
            lfo_depth: 0.0,
            master_gain: 0.2,
            attack: 0.01,
            decay: 0.2,
            sustain: 0.7,
            release: 0.3,
        }
    }

    pub fn oscillators(mut self, oscs: Vec<Oscillator>) -> Self {
        self.oscillators = oscs;
        self
    }

    pub fn add_osc(mut self, osc: Oscillator) -> Self {
        self.oscillators.push(osc);
        self
    }

    pub fn cutoff(mut self, v: f32) -> Self {
        self.cutoff = v;
        self
    }

    pub fn resonance(mut self, v: f32) -> Self {
        self.resonance = v;
        self
    }

    pub fn filter_type(mut self, v: FilterType) -> Self {
        self.filter_type = v;
        self
    }

    pub fn lfo_rate(mut self, v: f32) -> Self {
        self.lfo_rate = v;
        self
    }

    pub fn lfo_depth(mut self, v: f32) -> Self {
        self.lfo_depth = v;
        self
    }

    pub fn master_gain(mut self, v: f32) -> Self {
        self.master_gain = v;
        self
    }

    pub fn attack(mut self, v: f32) -> Self {
        self.attack = v;
        self
    }

    pub fn decay(mut self, v: f32) -> Self {
        self.decay = v;
        self
    }

    pub fn sustain(mut self, v: f32) -> Self {
        self.sustain = v;
        self
    }

    pub fn release(mut self, v: f32) -> Self {
        self.release = v;
        self
    }
}
