//! Buses: summing points with their own gain, chain and route. Tracks route or
//! send into them; the engine processes them in dependency order.

use crate::dsp::StereoFrame;
use crate::dsp::effects::FxCtx;
use crate::dsp::smooth::Smoothed;
use crate::engine::CONTROL_BLOCK;
use crate::engine::chain::{FxChain, NodeUpdate, SendTarget};
use crate::music::signal::{EvalCtx, Program, VoiceInputs};

pub struct Bus {
    pub name: String,
    pub input: Vec<StereoFrame>,
    pub route: Option<String>,
    chain: FxChain,
    gain: Program,
    level: Smoothed,
    mute: bool,
}

pub enum BusChain {
    New(FxChain),
    Update(Vec<NodeUpdate>),
}

/// One bus in a `SetBuses` command. The list arrives in processing order:
/// every bus comes before any bus it routes or sends to.
pub struct BusUpdate {
    pub name: String,
    pub gain: Program,
    pub mute: bool,
    pub route: Option<String>,
    pub chain: BusChain,
}

impl Bus {
    pub fn new(u: BusUpdate, chain: FxChain, sample_rate: f32) -> Self {
        // Start the fader where it should be, so a muted bus never leaks.
        let voice = VoiceInputs::default();
        let g0 = if u.mute { 0.0 } else { u.gain.eval(&EvalCtx { beat: 0.0, knobs: &[0.0; 128], voice: &voice }).max(0.0) };
        Bus {
            name: u.name,
            input: vec![[0.0; 2]; CONTROL_BLOCK],
            route: u.route,
            chain,
            gain: u.gain,
            level: Smoothed::new(g0, 8.0, sample_rate),
            mute: u.mute,
        }
    }

    pub fn update(&mut self, u: BusUpdate) {
        self.gain = u.gain;
        self.mute = u.mute;
        self.route = u.route;
        match u.chain {
            BusChain::New(c) => self.chain = c,
            BusChain::Update(ups) => self.chain.apply(ups),
        }
    }

    /// Run the chain, then the fader, over the first `n` input frames. Sends may only
    /// reach buses later in the order (`later`).
    pub fn process(&mut self, n: usize, ctx: &FxCtx, knobs: &[f32; 128], later: &mut [Bus]) {
        let voice = VoiceInputs::default();
        let g = self.gain.eval(&EvalCtx { beat: ctx.beat, knobs, voice: &voice });
        self.level.set_target(if self.mute { 0.0 } else { g.max(0.0) });
        let buf = &mut self.input[..n];
        self.chain.process(buf, ctx, knobs, later);
        for f in buf.iter_mut() {
            let g = self.level.next();
            f[0] *= g;
            f[1] *= g;
        }
    }
}

impl SendTarget for [Bus] {
    fn send(&mut self, bus: &str, buf: &[StereoFrame], gain: f32) {
        if let Some(b) = self.iter_mut().find(|b| b.name == bus) {
            for (d, s) in b.input.iter_mut().zip(buf) {
                d[0] += s[0] * gain;
                d[1] += s[1] * gain;
            }
        }
    }
}
