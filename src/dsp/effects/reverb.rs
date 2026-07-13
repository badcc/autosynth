use crate::dsp::StereoFrame;
use crate::dsp::effects::Effect;
use crate::model::fx::reverb_slots::{PARAM_DAMP, PARAM_MIX, PARAM_SIZE, PARAM_WIDTH};

// Freeverb tunings, in samples at 44.1 kHz. Scaled to the actual rate.
const COMB_TUNINGS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLPASS_TUNINGS: [usize; 4] = [556, 441, 341, 225];
const STEREO_SPREAD: usize = 23;
const FIXED_GAIN: f32 = 0.015;

struct Comb {
    buffer: Vec<f32>,
    pos: usize,
    filter_store: f32,
    feedback: f32,
    damp1: f32,
    damp2: f32,
}

impl Comb {
    fn new(size: usize) -> Self {
        Self {
            buffer: vec![0.0; size.max(1)],
            pos: 0,
            filter_store: 0.0,
            feedback: 0.5,
            damp1: 0.5,
            damp2: 0.5,
        }
    }

    fn set_damp(&mut self, d: f32) {
        self.damp1 = d;
        self.damp2 = 1.0 - d;
    }

    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        let output = self.buffer[self.pos];
        self.filter_store = output * self.damp2 + self.filter_store * self.damp1;
        self.buffer[self.pos] = input + self.filter_store * self.feedback;
        self.pos = (self.pos + 1) % self.buffer.len();
        output
    }

    fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.filter_store = 0.0;
    }
}

struct Allpass {
    buffer: Vec<f32>,
    pos: usize,
}

impl Allpass {
    fn new(size: usize) -> Self {
        Self {
            buffer: vec![0.0; size.max(1)],
            pos: 0,
        }
    }

    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        let bufout = self.buffer[self.pos];
        let output = -input + bufout;
        self.buffer[self.pos] = input + bufout * 0.5;
        self.pos = (self.pos + 1) % self.buffer.len();
        output
    }

    fn reset(&mut self) {
        self.buffer.fill(0.0);
    }
}

/// Freeverb-style stereo reverb: eight damped comb filters into four allpass
/// diffusers per channel.
pub struct Reverb {
    combs_l: Vec<Comb>,
    combs_r: Vec<Comb>,
    allpass_l: Vec<Allpass>,
    allpass_r: Vec<Allpass>,
    size: f32,
    damp: f32,
    mix: f32,
    width: f32,
}

impl Reverb {
    pub fn new(size: f32, damp: f32, mix: f32, width: f32, sample_rate: f32) -> Self {
        let scale = sample_rate / 44100.0;
        let s = |n: usize| ((n as f32) * scale) as usize;
        let mut rv = Self {
            combs_l: COMB_TUNINGS.iter().map(|&t| Comb::new(s(t))).collect(),
            combs_r: COMB_TUNINGS
                .iter()
                .map(|&t| Comb::new(s(t + STEREO_SPREAD)))
                .collect(),
            allpass_l: ALLPASS_TUNINGS.iter().map(|&t| Allpass::new(s(t))).collect(),
            allpass_r: ALLPASS_TUNINGS
                .iter()
                .map(|&t| Allpass::new(s(t + STEREO_SPREAD)))
                .collect(),
            size: 0.0,
            damp: 0.0,
            mix: mix.clamp(0.0, 1.0),
            width: width.clamp(0.0, 1.0),
        };
        rv.set_size(size);
        rv.set_damp(damp);
        rv
    }

    fn set_size(&mut self, size: f32) {
        self.size = size.clamp(0.0, 1.0);
        let feedback = 0.7 + self.size * 0.28; // roomsize 0.7..0.98
        for c in self.combs_l.iter_mut().chain(self.combs_r.iter_mut()) {
            c.feedback = feedback;
        }
    }

    fn set_damp(&mut self, damp: f32) {
        self.damp = damp.clamp(0.0, 1.0);
        let d = self.damp * 0.4;
        for c in self.combs_l.iter_mut().chain(self.combs_r.iter_mut()) {
            c.set_damp(d);
        }
    }
}

impl Effect for Reverb {
    fn process(&mut self, frame: StereoFrame, _sample_rate: f32) -> StereoFrame {
        let input = (frame[0] + frame[1]) * FIXED_GAIN;

        let mut out_l = 0.0;
        let mut out_r = 0.0;
        for c in &mut self.combs_l {
            out_l += c.process(input);
        }
        for c in &mut self.combs_r {
            out_r += c.process(input);
        }
        for a in &mut self.allpass_l {
            out_l = a.process(out_l);
        }
        for a in &mut self.allpass_r {
            out_r = a.process(out_r);
        }

        // Stereo width: blend the two wet channels.
        let wet1 = self.width * 0.5 + 0.5;
        let wet2 = (1.0 - self.width) * 0.5;
        let wet_l = out_l * wet1 + out_r * wet2;
        let wet_r = out_r * wet1 + out_l * wet2;

        [
            frame[0] * (1.0 - self.mix) + wet_l * self.mix,
            frame[1] * (1.0 - self.mix) + wet_r * self.mix,
        ]
    }

    fn reset(&mut self) {
        for c in self.combs_l.iter_mut().chain(self.combs_r.iter_mut()) {
            c.reset();
        }
        for a in self.allpass_l.iter_mut().chain(self.allpass_r.iter_mut()) {
            a.reset();
        }
    }

    fn set_param(&mut self, slot: u8, value: f32) {
        match slot {
            PARAM_SIZE => self.set_size(value),
            PARAM_DAMP => self.set_damp(value),
            PARAM_MIX => self.mix = value.clamp(0.0, 1.0),
            PARAM_WIDTH => self.width = value.clamp(0.0, 1.0),
            _ => {}
        }
    }
}
