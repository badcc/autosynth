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
