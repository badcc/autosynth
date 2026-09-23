use crate::dsp::StereoFrame;
use crate::dsp::effects::{Effect, FxCtx, ParamDef, mix};

pub const TIME: usize = 0;
pub const FEEDBACK: usize = 1;
pub const MIX: usize = 2;

pub const PARAMS: &[ParamDef] = &[
    ParamDef { name: "time", default: 0.5 },
    ParamDef { name: "feedback", default: 0.3 },
    ParamDef { name: "mix", default: 0.3 },
];

/// Longest delay the buffer holds, in seconds.
const MAX_SECONDS: f32 = 4.0;

/// Delay feedback routing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DelayMode {
    /// Each channel feeds back into itself.
    Normal,
    /// Input → left → right → left …: echoes alternate sides.
    PingPong,
}

/// Tempo-synced stereo feedback delay. `time` is in beats and follows tempo
/// changes live; the read head glides to a new time (tape-style) instead of
/// clicking.
pub struct Delay {
    buf: [Vec<f32>; 2],
    write: usize,
    /// Current delay in samples (smoothed toward the target).
    current: f32,
    time: f32,
    feedback: f32,
    mix: f32,
    mode: DelayMode,
}

impl Delay {
    pub fn new(sample_rate: f32, mode: DelayMode) -> Self {
        let len = (MAX_SECONDS * sample_rate) as usize + 2;
        Self {
            buf: [vec![0.0; len], vec![0.0; len]],
            write: 0,
            current: -1.0,
            time: PARAMS[TIME].default,
            feedback: PARAMS[FEEDBACK].default,
            mix: PARAMS[MIX].default,
            mode,
        }
    }

    #[inline]
    fn read(&self, ch: usize, delay: f32) -> f32 {
        let len = self.buf[ch].len();
        let pos = self.write as f32 - delay;
        let pos = if pos < 0.0 { pos + len as f32 } else { pos };
        let i = pos as usize % len;
        let frac = pos - pos.floor();
        let j = (i + 1) % len;
        self.buf[ch][i] * (1.0 - frac) + self.buf[ch][j] * frac
    }
}

impl Effect for Delay {
    fn process(&mut self, buf: &mut [StereoFrame], ctx: &FxCtx) {
        let len = self.buf[0].len();
        let target = (self.time * 60.0 / ctx.bpm.max(1.0) * ctx.sample_rate).clamp(1.0, (len - 2) as f32);
        if self.current < 0.0 {
            self.current = target;
        }
        let glide = 1.0 - (-1.0 / (0.05 * ctx.sample_rate)).exp();
        for frame in buf.iter_mut() {
            self.current += (target - self.current) * glide;
            let (dl, dr) = (self.read(0, self.current), self.read(1, self.current));
            match self.mode {
                DelayMode::Normal => {
                    self.buf[0][self.write] = frame[0] + dl * self.feedback;
                    self.buf[1][self.write] = frame[1] + dr * self.feedback;
                }
                DelayMode::PingPong => {
                    self.buf[0][self.write] = (frame[0] + frame[1]) * 0.5 + dr * self.feedback;
                    self.buf[1][self.write] = dl;
                }
            }
            self.write = (self.write + 1) % len;
            frame[0] = mix(frame[0], dl, self.mix);
            frame[1] = mix(frame[1], dr, self.mix);
        }
    }

    fn set(&mut self, slot: usize, v: f32) {
        match slot {
            TIME => self.time = v.max(0.0),
            FEEDBACK => self.feedback = v.clamp(0.0, 0.98),
            MIX => self.mix = v.clamp(0.0, 1.0),
            _ => {}
        }
    }

    fn reset(&mut self) {
        self.buf[0].fill(0.0);
        self.buf[1].fill(0.0);
    }
}
