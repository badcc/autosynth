use std::sync::mpsc;

use crate::dsp::StereoFrame;
use crate::dsp::effects::FxCtx;
use crate::engine::CONTROL_BLOCK;
use crate::engine::bus::{Bus, BusChain};
use crate::engine::chain::FxChain;
use crate::engine::command::{Command, EngineHandle};
use crate::engine::mixer::soft_limit;
use crate::engine::track::{BlockCtx, Track};
use crate::engine::transport::Transport;
use crate::music::harmony::MAJOR;
use crate::music::notes::C4;
use crate::music::pitch::Key;

/// The real-time engine. Renders into any interleaved slice — cpal in the live
/// app, a plain buffer for offline render and tests.
pub struct Engine {
    rx: mpsc::Receiver<Command>,
    /// Kept in insertion order so mixing (and so rendering) is deterministic.
    tracks: Vec<Track>,
    /// In processing order: every bus precedes the buses it feeds.
    buses: Vec<Bus>,
    master: FxChain,
    transport: Transport,
    channels: usize,
    sample_rate: f32,
    key: Key,
    knobs: [f32; 128],
    midi_track: Option<String>,
    master_buf: Vec<StereoFrame>,
    scratch: Vec<usize>,
}

impl Engine {
    pub fn new(sample_rate: f32, channels: usize, bpm: f32) -> (Self, EngineHandle) {
        let (tx, rx) = mpsc::channel();
        let engine = Self {
            rx,
            tracks: Vec::new(),
            buses: Vec::new(),
            master: FxChain::default(),
            transport: Transport::new(bpm, sample_rate),
            channels,
            sample_rate,
            key: Key::new(C4, MAJOR),
            knobs: [0.0; 128],
            midi_track: None,
            master_buf: vec![[0.0; 2]; CONTROL_BLOCK],
            scratch: Vec::with_capacity(64),
        };
        (engine, EngineHandle::new(tx))
    }

    pub fn transport(&self) -> &Transport {
        &self.transport
    }

    /// Build a cpal output stream, moving the engine into the audio callback.
    pub fn build_stream(self, device: &cpal::Device, config: &cpal::StreamConfig) -> Result<cpal::Stream, cpal::BuildStreamError> {
        use cpal::traits::DeviceTrait;
        let mut engine = self;
        device.build_output_stream(
            config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| engine.render(data),
            |err| eprintln!("audio stream error: {err}"),
            None,
        )
    }

    /// Render into an interleaved output buffer, draining commands first.
    pub fn render(&mut self, output: &mut [f32]) {
        while let Ok(cmd) = self.rx.try_recv() {
            self.apply(cmd);
        }
        let frames = output.len() / self.channels;
        let mut done = 0;
        while done < frames {
            let n = (frames - done).min(CONTROL_BLOCK);
            let (a, b) = (done * self.channels, (done + n) * self.channels);
            self.render_block(&mut output[a..b], n);
            done += n;
        }
    }

    fn render_block(&mut self, out: &mut [f32], n: usize) {
        self.transport.update();
        let b0 = self.transport.beat;
        let bps = self.transport.beats_per_sample();
        let ctx = BlockCtx {
            b0,
            b1: self.transport.beat_at(n),
            bps,
            transport: &self.transport,
            knobs: &self.knobs,
            key: self.key,
            fx: FxCtx {
                sample_rate: self.sample_rate,
                bpm: self.transport.bpm as f32,
                beat: self.transport.song(b0),
                beats_per_sample: bps,
            },
        };

        self.master_buf[..n].fill([0.0; 2]);
        for bus in &mut self.buses {
            bus.input[..n].fill([0.0; 2]);
        }

        // 1. Signals, pattern regeneration, event collection.
        for t in &mut self.tracks {
            t.prepare(&ctx, n);
        }

        // 2. Sidechain: forward each source's note-on offsets to its duckers.
        for i in 0..self.tracks.len() {
            let Some(src) = self.tracks[i].duck_source() else {
                continue;
            };
            self.scratch.clear();
            if let Some(s) = self.tracks.iter().find(|t| t.name == src) {
                self.scratch.extend_from_slice(s.onsets());
            }
            self.tracks[i].set_duck_triggers(&self.scratch);
        }

        // 3. Tracks → their route (a bus input, or master).
        for t in &mut self.tracks {
            t.render(&ctx, n, &mut self.buses);
            let dest = match &t.route {
                Some(bus) => self.buses.iter_mut().find(|b| &b.name == bus).map(|b| &mut b.input[..n]),
                None => None,
            };
            add(dest.unwrap_or(&mut self.master_buf[..n]), t.out(n));
        }

        // 4. Buses in order; each may feed only buses after it.
        for i in 0..self.buses.len() {
            let (head, later) = self.buses.split_at_mut(i + 1);
            let bus = &mut head[i];
            bus.process(n, &ctx.fx, &self.knobs, later);
            let dest = match &bus.route {
                Some(r) => later.iter_mut().find(|b| &b.name == r).map(|b| &mut b.input[..n]),
                None => None,
            };
            add(dest.unwrap_or(&mut self.master_buf[..n]), &bus.input[..n]);
        }

        // 5. Master chain, then the safety soft-limiter.
        let no_buses: &mut [Bus] = &mut [];
        self.master.process(&mut self.master_buf[..n], &ctx.fx, &self.knobs, no_buses);
        for (i, frame) in self.master_buf[..n].iter().enumerate() {
            let (l, r) = (soft_limit(frame[0]), soft_limit(frame[1]));
            let o = i * self.channels;
            match self.channels {
                1 => out[o] = (l + r) * 0.5,
                _ => {
                    out[o] = l;
                    out[o + 1] = r;
                    out[o + 2..o + self.channels].fill(0.0);
                }
            }
        }

        self.transport.advance(n);
    }

    fn track(&mut self, name: &str) -> Option<&mut Track> {
        self.tracks.iter_mut().find(|t| t.name == name)
    }

    fn apply(&mut self, cmd: Command) {
        match cmd {
            Command::AddTrack(build) => {
                // New tracks launch on the next bar line.
                let now = self.transport.next_bar();
                let t = Track::new(*build, now, &self.transport, &self.knobs, self.key, self.sample_rate);
                match self.tracks.iter_mut().find(|x| x.name == t.name) {
                    Some(slot) => *slot = t,
                    None => self.tracks.push(t),
                }
            }
            Command::RemoveTrack(name) => self.tracks.retain(|t| t.name != name),
            Command::SetInstrument { track, cfg } => {
                if let Some(t) = self.track(&track) {
                    t.set_instrument(*cfg);
                }
            }
            Command::SetMixer { track, gain, pan, mute } => {
                if let Some(t) = self.track(&track) {
                    t.set_mixer(gain, pan, mute);
                }
            }
            Command::SetChain { track, updates } => {
                if let Some(t) = self.track(&track) {
                    t.set_chain(updates);
                }
            }
            Command::SetRoute { track, route } => {
                if let Some(t) = self.track(&track) {
                    t.route = route;
                }
            }
            Command::SetDuck { track, duck } => {
                if let Some(t) = self.track(&track) {
                    t.set_duck(duck);
                }
            }
            Command::SetTrackKey { track, key } => {
                if let Some(t) = self.track(&track) {
                    t.set_key(key);
                }
            }
            Command::QueuePattern { track, pattern } => {
                if let Some(t) = self.track(&track) {
                    t.queue_pattern(pattern);
                }
            }
            Command::SetBuses(updates) => {
                let mut old = std::mem::take(&mut self.buses);
                for u in updates {
                    let existing = old.iter().position(|b| b.name == u.name).map(|i| old.swap_remove(i));
                    let bus = match (existing, u.chain) {
                        (Some(mut b), chain) => {
                            b.update(crate::engine::bus::BusUpdate { chain, ..u });
                            b
                        }
                        (None, BusChain::New(c)) => Bus::new(crate::engine::bus::BusUpdate { chain: BusChain::Update(Vec::new()), ..u }, c, self.sample_rate),
                        (None, BusChain::Update(_)) => {
                            tracing::error!("bus `{}` out of sync with the scene", u.name);
                            continue;
                        }
                    };
                    self.buses.push(bus);
                }
            }
            Command::SetMaster(updates) => self.master.apply(updates),
            Command::SetTempo(bpm) => self.transport.bpm = bpm as f64,
            Command::SetKey(key) => self.key = key,
            Command::Jump(beat) => self.transport.jump(beat),
            Command::Hold(span) => self.transport.hold(span),
            Command::MidiNoteOn { note, vel } => {
                if let Some(name) = self.midi_track.clone()
                    && let Some(t) = self.track(&name)
                {
                    t.note_on(note, vel);
                }
            }
            Command::MidiNoteOff { note } => {
                if let Some(name) = self.midi_track.clone()
                    && let Some(t) = self.track(&name)
                {
                    t.note_off(note);
                }
            }
            Command::MidiCc { cc, value } => self.knobs[(cc & 127) as usize] = value.clamp(0.0, 1.0),
            Command::SetMidiTrack(name) => self.midi_track = name,
        }
    }
}

fn add(dest: &mut [StereoFrame], src: &[StereoFrame]) {
    for (d, s) in dest.iter_mut().zip(src) {
        d[0] += s[0];
        d[1] += s[1];
    }
}
