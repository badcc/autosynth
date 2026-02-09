use crate::effects::{Effect, StereoFrame};

pub const PARAM_FEEDBACK: u8 = 0;
pub const PARAM_MIX: u8 = 1;

/// Delay feedback routing mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DelayMode {
    /// Each channel feeds back into itself independently.
    Normal,
    /// Serial chain: input → L buffer → R buffer → L buffer → ...
    /// Echo onsets alternate L/R, decaying by feedback each round trip.
    PingPong,
}

/// Stereo feedback delay with optional ping-pong mode.
pub struct Delay {
    buffers: [Vec<f32>; 2],
    write_pos: usize,
    delay_samples: usize,
    feedback: f32,
    mix: f32,
    mode: DelayMode,
}

impl Delay {
    /// Create a new delay effect.
    ///
    /// `delay_time`: delay time in seconds
    /// `feedback`: feedback amount (0.0 to ~0.95)
    /// `mix`: dry/wet mix (0.0 = fully dry, 1.0 = fully wet)
    /// `sample_rate`: audio sample rate in Hz
    pub fn new(delay_time: f32, feedback: f32, mix: f32, sample_rate: f32) -> Self {
        let delay_samples = (delay_time * sample_rate) as usize;
        let buffer_size = delay_samples.max(1);
        Self {
            buffers: [vec![0.0; buffer_size], vec![0.0; buffer_size]],
            write_pos: 0,
            delay_samples: buffer_size,
            feedback: feedback.clamp(0.0, 0.95),
            mix: mix.clamp(0.0, 1.0),
            mode: DelayMode::Normal,
        }
    }

    /// Create a tempo-synced delay.
    ///
    /// `delay_beats`: delay time in beats
    /// `bpm`: tempo in beats per minute
    /// `feedback`: feedback amount (0.0 to ~0.95)
    /// `mix`: dry/wet mix (0.0 = fully dry, 1.0 = fully wet)
    /// `sample_rate`: audio sample rate in Hz
    pub fn tempo_synced(
        delay_beats: f32,
        bpm: f32,
        feedback: f32,
        mix: f32,
        sample_rate: f32,
    ) -> Self {
        let delay_seconds = delay_beats * 60.0 / bpm;
        Self::new(delay_seconds, feedback, mix, sample_rate)
    }

    /// Set the delay mode (Normal or PingPong).
    pub fn mode(mut self, mode: DelayMode) -> Self {
        self.mode = mode;
        self
    }
}

impl Effect for Delay {
    fn process(&mut self, frame: StereoFrame, _sample_rate: f32) -> StereoFrame {
        let len = self.buffers[0].len();
        let read_pos = (self.write_pos + len - self.delay_samples) % len;

        let delayed_l = self.buffers[0][read_pos];
        let delayed_r = self.buffers[1][read_pos];

        match self.mode {
            DelayMode::Normal => {
                self.buffers[0][self.write_pos] = frame[0] + delayed_l * self.feedback;
                self.buffers[1][self.write_pos] = frame[1] + delayed_r * self.feedback;
            }
            DelayMode::PingPong => {
                // Input feeds L. L's output feeds R. R's output feeds back to L with feedback.
                // Produces alternating echo onsets: L at 1T, R at 2T, L at 3T, ...
                let input_mono = (frame[0] + frame[1]) * 0.5;
                self.buffers[0][self.write_pos] = input_mono + delayed_r * self.feedback;
                self.buffers[1][self.write_pos] = delayed_l;
            }
        }

        self.write_pos = (self.write_pos + 1) % len;

        [
            frame[0] * (1.0 - self.mix) + delayed_l * self.mix,
            frame[1] * (1.0 - self.mix) + delayed_r * self.mix,
        ]
    }

    fn reset(&mut self) {
        self.buffers[0].fill(0.0);
        self.buffers[1].fill(0.0);
        self.write_pos = 0;
    }

    fn set_param(&mut self, slot: u8, value: f32) {
        match slot {
            PARAM_FEEDBACK => self.feedback = value.clamp(0.0, 0.95),
            PARAM_MIX => self.mix = value.clamp(0.0, 1.0),
            _ => {}
        }
    }
}
