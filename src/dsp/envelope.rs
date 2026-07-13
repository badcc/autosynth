/// Controls how a voice behaves when retriggered (voice stealing or same-note retrigger).
///
/// Only affects voices that are already active. Fresh voices always start clean.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[derive(Default)]
pub enum RetriggerMode {
    /// Reset envelope to zero, restart attack. Reset oscillator phase and filter.
    /// Percussive and distinct, but clicks when stealing an active voice.
    Hard,
    /// Restart attack from the current envelope level. Preserve oscillator phase
    /// and filter state — no discontinuity in the waveform or amplitude.
    /// Each note still gets its own attack transient, just without the click.
    #[default]
    Soft,
    /// If the voice is still in its note-on phase (attack/decay/sustain), just change
    /// pitch — don't retrigger the envelope or touch oscillators/filter at all.
    /// If in release or idle, behaves like Soft (attack from current level, keep phase).
    /// Classic mono-synth legato feel.
    Legato,
}


#[derive(Clone, Copy, Debug)]
pub enum EnvState {
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}

#[derive(Clone, Copy, Debug)]
pub struct Adsr {
    pub state: EnvState,
    pub level: f32,
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
    release_start: f32,
}

impl Default for Adsr {
    fn default() -> Self {
        Self::new()
    }
}

impl Adsr {
    pub fn new() -> Self {
        Self {
            state: EnvState::Idle,
            level: 0.0,
            attack: 0.01,
            decay: 0.2,
            sustain: 0.7,
            release: 0.3,
            release_start: 0.0,
        }
    }

    pub fn note_on(&mut self) {
        self.state = EnvState::Attack;
        self.level = 0.0;
    }

    /// Retrigger the envelope on an already-active voice.
    /// Returns `true` if oscillators/filter should be hard-reset (only for `Hard` mode).
    /// Soft and Legato preserve oscillator phase to avoid waveform discontinuities.
    pub fn retrigger(&mut self, mode: RetriggerMode) -> bool {
        match mode {
            RetriggerMode::Hard => {
                self.state = EnvState::Attack;
                self.level = 0.0;
                true
            }
            RetriggerMode::Soft => {
                // Keep current level, ramp to 1.0 at the normal attack rate.
                // Oscillator phase and filter stay untouched.
                self.state = EnvState::Attack;
                false
            }
            RetriggerMode::Legato => match self.state {
                // Voice is still in note-on phase — don't retrigger, just let
                // the caller change pitch. Everything stays untouched.
                EnvState::Attack | EnvState::Decay | EnvState::Sustain => false,
                // Voice was fading or dead — soft retrigger from current level.
                EnvState::Release | EnvState::Idle => {
                    self.state = EnvState::Attack;
                    false
                }
            },
        }
    }

    pub fn note_off(&mut self) {
        self.state = EnvState::Release;
        self.release_start = self.level.max(0.0);
    }

    pub fn is_active(&self) -> bool {
        !matches!(self.state, EnvState::Idle)
    }

    pub fn next_sample(&mut self, dt: f32) -> f32 {
        match self.state {
            EnvState::Idle => {
                self.level = 0.0;
            }
            EnvState::Attack => {
                if self.attack <= 0.0 {
                    self.level = 1.0;
                } else {
                    self.level += dt / self.attack;
                }
                if self.level >= 1.0 {
                    self.level = 1.0;
                    self.state = EnvState::Decay;
                }
            }
            EnvState::Decay => {
                if self.decay <= 0.0 {
                    self.level = self.sustain;
                } else {
                    let drop = (1.0 - self.sustain).max(0.0);
                    self.level -= dt * drop / self.decay;
                }
                if self.level <= self.sustain {
                    self.level = self.sustain;
                    self.state = EnvState::Sustain;
                }
            }
            EnvState::Sustain => {
                self.level = self.sustain;
            }
            EnvState::Release => {
                if self.release <= 0.0 {
                    self.level = 0.0;
                } else {
                    self.level -= dt * self.release_start / self.release;
                }
                if self.level <= 0.0 {
                    self.level = 0.0;
                    self.state = EnvState::Idle;
                }
            }
        }
        self.level
    }
}
