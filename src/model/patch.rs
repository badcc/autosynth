use crate::dsp::envelope::RetriggerMode;
use crate::dsp::filter::FilterType;
use crate::dsp::oscillator::Oscillator;
use crate::dsp::waveform::Waveform;

/// The diffable description of a synth voice: oscillators, envelope, filter, and
/// LFO. Plain data — the builder in `live` fills it, the diff compares it, and
/// the engine turns it into a running `VoiceBank`. Master gain lives on the
/// mixer, not here.
#[derive(Clone, Debug, PartialEq)]
pub struct PatchSpec {
    pub oscillators: Vec<Oscillator>,
    pub cutoff: f32,
    pub resonance: f32,
    pub filter_type: FilterType,
    pub lfo_rate: f32,
    pub lfo_depth: f32,
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
    pub retrigger: RetriggerMode,
}

impl Default for PatchSpec {
    fn default() -> Self {
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
            attack: 0.01,
            decay: 0.2,
            sustain: 0.7,
            release: 0.3,
            retrigger: RetriggerMode::default(),
        }
    }
}
