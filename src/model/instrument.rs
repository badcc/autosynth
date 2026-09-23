//! Instruments as values. `Synth::new().osc(Saw.unison(7, 0.2)).ladder(..)`
//! builds a plain, comparable description; presets are just functions that
//! return one, and tweaking a preset is chaining more calls onto it.

use std::path::PathBuf;

use crate::dsp::envelope::RetriggerMode;
use crate::dsp::filter::FilterType;
use crate::dsp::waveform::Waveform;
use crate::music::signal::{EnvShape, Mod, vel};

/// One oscillator: waveform, per-voice level and pitch offset (both [`Mod`]s),
/// a unison stack, and a start phase.
#[derive(Clone, Debug, PartialEq)]
pub struct OscSpec {
    pub wave: Waveform,
    pub level: Mod,
    /// Pitch offset in semitones.
    pub semis: Mod,
    pub unison: u8,
    /// Total unison detune width, semitones.
    pub spread: f32,
    pub phase: f32,
}

impl From<Waveform> for OscSpec {
    fn from(wave: Waveform) -> Self {
        OscSpec { wave, level: Mod::constant(1.0), semis: Mod::constant(0.0), unison: 1, spread: 0.0, phase: 0.0 }
    }
}

impl OscSpec {
    pub fn level(mut self, m: impl Into<Mod>) -> Self {
        self.level = m.into();
        self
    }

    /// Pitch offset in semitones (fractional detunes, or `lfo(5.0) * 0.2`
    /// for vibrato).
    pub fn semis(mut self, m: impl Into<Mod>) -> Self {
        self.semis = m.into();
        self
    }

    /// Stack `voices` detuned copies spread over `spread` semitones.
    pub fn unison(mut self, voices: u8, spread: f32) -> Self {
        self.unison = voices.max(1);
        self.spread = spread;
        self
    }

    /// Start phase, `0..1`.
    pub fn phase(mut self, p: f32) -> Self {
        self.phase = p;
        self
    }
}

impl Waveform {
    pub fn level(self, m: impl Into<Mod>) -> OscSpec {
        OscSpec::from(self).level(m)
    }

    pub fn semis(self, m: impl Into<Mod>) -> OscSpec {
        OscSpec::from(self).semis(m)
    }

    pub fn unison(self, voices: u8, spread: f32) -> OscSpec {
        OscSpec::from(self).unison(voices, spread)
    }

    pub fn phase(self, p: f32) -> OscSpec {
        OscSpec::from(self).phase(p)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FilterSpec {
    pub kind: FilterType,
    pub cutoff: Mod,
    pub res: Mod,
}

/// Where a voice's raw sound comes from.
#[derive(Clone, Debug, PartialEq)]
pub enum SourceSpec {
    Osc(Vec<OscSpec>),
    /// A pitched sample: `root` plays at native speed.
    Sample { path: PathBuf, root: u8 },
    /// A drum kit: each note plays its own sample at native pitch.
    Kit(Vec<(u8, PathBuf)>),
}

impl SourceSpec {
    /// Same kind of source (changing kind rebuilds voices).
    pub fn same_kind(&self, other: &SourceSpec) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }
}

/// The complete description of an instrument: source, filter, amp envelope,
/// modulation, and voicing.
#[derive(Clone, Debug, PartialEq)]
pub struct InstrumentSpec {
    pub source: SourceSpec,
    pub filter: Option<FilterSpec>,
    pub amp: EnvShape,
    /// Voice output gain (default: velocity).
    pub gain: Mod,
    /// Pitch offset in semitones for the whole voice.
    pub pitch: Mod,
    pub voices: usize,
    pub mono: bool,
    /// Portamento time in seconds (mono voices).
    pub glide: f32,
    pub retrigger: RetriggerMode,
    /// Ignore note-off: play until the sample ends (drum kits).
    pub oneshot: bool,
}

impl InstrumentSpec {
    fn new(source: SourceSpec) -> Self {
        let oneshot = matches!(source, SourceSpec::Kit(_));
        InstrumentSpec {
            source,
            filter: None,
            amp: EnvShape { attack: 0.005, decay: 0.2, sustain: 0.7, release: 0.2 },
            gain: vel(),
            pitch: Mod::constant(0.0),
            voices: 8,
            mono: false,
            glide: 0.0,
            retrigger: RetriggerMode::default(),
            oneshot,
        }
    }

    /// A default drum kit with the given slots.
    pub fn kit(slots: Vec<(u8, PathBuf)>) -> Self {
        Sampler::kit().0.with_slots(slots)
    }

    /// Replace a kit's slots (no-op on other sources).
    pub fn with_slots(mut self, slots: Vec<(u8, PathBuf)>) -> Self {
        if let SourceSpec::Kit(s) = &mut self.source {
            *s = slots;
        }
        self
    }
}

impl Default for InstrumentSpec {
    fn default() -> Self {
        Synth::new().into()
    }
}

/// A subtractive synth voice: oscillators → filter → amp envelope.
#[derive(Clone, Debug, PartialEq)]
pub struct Synth(InstrumentSpec);

/// A sample player: pitched (`Sampler::new(path)`) or a kit (`Sampler::kit()`,
/// slots added with `t.slot(path)`).
#[derive(Clone, Debug, PartialEq)]
pub struct Sampler(InstrumentSpec);

impl From<Synth> for InstrumentSpec {
    fn from(s: Synth) -> Self {
        s.0
    }
}

impl From<Sampler> for InstrumentSpec {
    fn from(s: Sampler) -> Self {
        s.0
    }
}

impl Default for Synth {
    fn default() -> Self {
        Self::new()
    }
}

impl Synth {
    /// An empty synth; with no `.osc(..)` it plays a single saw.
    pub fn new() -> Self {
        Synth(InstrumentSpec::new(SourceSpec::Osc(Vec::new())))
    }

    /// Add an oscillator: a bare waveform (`Saw`) or a configured one
    /// (`Saw.unison(7, 0.2).level(0.5)`).
    pub fn osc(mut self, osc: impl Into<OscSpec>) -> Self {
        if let SourceSpec::Osc(oscs) = &mut self.0.source {
            oscs.push(osc.into());
        }
        self
    }

    pub fn spec(&self) -> &InstrumentSpec {
        &self.0
    }
}

impl Sampler {
    /// A pitched sample; `root` defaults to middle C.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Sampler(InstrumentSpec::new(SourceSpec::Sample { path: path.into(), root: 60 }))
    }

    /// A drum kit. Hits play to the end of the sample (see `.gated()`).
    pub fn kit() -> Self {
        let mut s = InstrumentSpec::new(SourceSpec::Kit(Vec::new()));
        s.amp = EnvShape { attack: 0.0005, decay: 0.0, sustain: 1.0, release: 0.05 };
        s.retrigger = RetriggerMode::Hard;
        Sampler(s)
    }

    /// The note that plays a pitched sample at its native speed.
    pub fn root(mut self, note: u8) -> Self {
        if let SourceSpec::Sample { root, .. } = &mut self.0.source {
            *root = note;
        }
        self
    }

    /// Respect note-off (release at the end of each note) instead of playing
    /// samples to the end.
    pub fn gated(mut self) -> Self {
        self.0.oneshot = false;
        self
    }

    pub fn spec(&self) -> &InstrumentSpec {
        &self.0
    }
}

/// Voice-shaping methods shared by `Synth` and `Sampler`.
macro_rules! voice_methods {
    ($t:ident) => {
        impl $t {
            fn filter(mut self, kind: FilterType, cutoff: impl Into<Mod>, res: impl Into<Mod>) -> Self {
                self.0.filter = Some(FilterSpec { kind, cutoff: cutoff.into(), res: res.into() });
                self
            }

            pub fn lowpass(self, cutoff: impl Into<Mod>, res: impl Into<Mod>) -> Self {
                self.filter(FilterType::Lowpass, cutoff, res)
            }

            pub fn highpass(self, cutoff: impl Into<Mod>, res: impl Into<Mod>) -> Self {
                self.filter(FilterType::Highpass, cutoff, res)
            }

            pub fn bandpass(self, cutoff: impl Into<Mod>, res: impl Into<Mod>) -> Self {
                self.filter(FilterType::Bandpass, cutoff, res)
            }

            pub fn notch(self, cutoff: impl Into<Mod>, res: impl Into<Mod>) -> Self {
                self.filter(FilterType::Notch, cutoff, res)
            }

            /// The 4-pole resonant ladder lowpass.
            pub fn ladder(self, cutoff: impl Into<Mod>, res: impl Into<Mod>) -> Self {
                self.filter(FilterType::Ladder, cutoff, res)
            }

            /// Replace the filter cutoff (adds a lowpass if there is none).
            pub fn cutoff(mut self, m: impl Into<Mod>) -> Self {
                match &mut self.0.filter {
                    Some(f) => f.cutoff = m.into(),
                    None => self.0.filter = Some(FilterSpec { kind: FilterType::Lowpass, cutoff: m.into(), res: Mod::constant(0.0) }),
                }
                self
            }

            /// Replace the filter resonance (adds a lowpass if there is none).
            pub fn res(mut self, m: impl Into<Mod>) -> Self {
                match &mut self.0.filter {
                    Some(f) => f.res = m.into(),
                    None => self.0.filter = Some(FilterSpec { kind: FilterType::Lowpass, cutoff: Mod::constant(20_000.0), res: m.into() }),
                }
                self
            }

            /// The amp envelope (seconds; sustain is a level).
            pub fn adsr(mut self, attack: f32, decay: f32, sustain: f32, release: f32) -> Self {
                self.0.amp = EnvShape { attack, decay, sustain, release };
                self
            }

            pub fn attack(mut self, s: f32) -> Self {
                self.0.amp.attack = s;
                self
            }

            pub fn decay(mut self, s: f32) -> Self {
                self.0.amp.decay = s;
                self
            }

            pub fn sustain(mut self, level: f32) -> Self {
                self.0.amp.sustain = level;
                self
            }

            pub fn release(mut self, s: f32) -> Self {
                self.0.amp.release = s;
                self
            }

            /// Voice output gain (default `vel()`).
            pub fn gain(mut self, m: impl Into<Mod>) -> Self {
                self.0.gain = m.into();
                self
            }

            /// Pitch offset in semitones — `env(0.0, 0.05) * 24.0` makes a kick.
            pub fn pitch(mut self, m: impl Into<Mod>) -> Self {
                self.0.pitch = m.into();
                self
            }

            /// Polyphony.
            pub fn voices(mut self, n: usize) -> Self {
                self.0.voices = n.max(1);
                self
            }

            /// One voice; overlapping notes play legato.
            pub fn mono(mut self) -> Self {
                self.0.mono = true;
                self
            }

            /// Portamento time in seconds (implies mono).
            pub fn glide(mut self, s: f32) -> Self {
                self.0.mono = true;
                self.0.glide = s.max(0.0);
                self
            }

            pub fn retrigger(mut self, mode: RetriggerMode) -> Self {
                self.0.retrigger = mode;
                self
            }
        }
    };
}

voice_methods!(Synth);
voice_methods!(Sampler);
