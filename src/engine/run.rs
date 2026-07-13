use std::collections::{HashMap, HashSet};
use std::sync::mpsc;

use crate::dsp::StereoFrame;
use crate::engine::command::{BuiltSource, Command, EngineHandle, GroupBuild, Playback, TrackBuild};
use crate::engine::mixer::{Bus, soft_limit};
use crate::engine::track::Track;
use crate::engine::transport::Transport;
use crate::engine::CONTROL_BLOCK;

/// The real-time engine. Renders block-by-block into any output slice — cpal in
/// the live app, a plain buffer for offline render and tests.
pub struct Engine {
    rx: mpsc::Receiver<Command>,
    tracks: HashMap<String, Track>,
    groups: Vec<Bus>,
    grouped: HashSet<String>,
    transport: Transport,
    channels: usize,
    sample_rate: f32,
    midi_track: Option<String>,
    master: Vec<StereoFrame>,
    group_buf: Vec<StereoFrame>,
}

impl Engine {
    pub fn new(sample_rate: f32, channels: usize, bpm: f32) -> (Self, EngineHandle) {
        let (tx, rx) = mpsc::channel();
        let engine = Self {
            rx,
            tracks: HashMap::new(),
            groups: Vec::new(),
            grouped: HashSet::new(),
            transport: Transport::new(bpm, sample_rate),
            channels,
            sample_rate,
            midi_track: None,
            master: vec![[0.0; 2]; CONTROL_BLOCK],
            group_buf: vec![[0.0; 2]; CONTROL_BLOCK],
        };
        (engine, EngineHandle::new(tx))
    }

    /// Build a cpal output stream, moving the engine into the audio callback.
    pub fn build_stream(
        self,
        device: &cpal::Device,
        config: &cpal::StreamConfig,
    ) -> Result<cpal::Stream, cpal::BuildStreamError> {
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
            let c = (frames - done).min(CONTROL_BLOCK);
            let start = done * self.channels;
            let end = (done + c) * self.channels;
            self.render_chunk(&mut output[start..end], c);
            done += c;
        }
    }

    fn render_chunk(&mut self, out: &mut [f32], c: usize) {
        let b0 = self.transport.beat;
        let b1 = self.transport.beat_at(c);
        let bps = self.transport.beats_per_sample();
        let sr = self.sample_rate;

        for f in &mut self.master[..c] {
            *f = [0.0; 2];
        }

        // Grouped tracks: render into the group buffer, run the bus, sum in.
        let mut groups = std::mem::take(&mut self.groups);
        for bus in &mut groups {
            for f in &mut self.group_buf[..c] {
                *f = [0.0; 2];
            }
            for member in &bus.members {
                if let Some(t) = self.tracks.get_mut(member) {
                    t.render_add(&mut self.group_buf[..c], b0, b1, bps);
                }
            }
            bus.process(&mut self.group_buf[..c], sr);
            for i in 0..c {
                self.master[i][0] += self.group_buf[i][0];
                self.master[i][1] += self.group_buf[i][1];
            }
        }
        self.groups = groups;

        // Ungrouped tracks go straight to master.
        for (name, t) in &mut self.tracks {
            if !self.grouped.contains(name) {
                t.render_add(&mut self.master[..c], b0, b1, bps);
            }
        }

        // Master soft limiter → interleaved output.
        for (i, frame) in self.master[..c].iter().enumerate() {
            let l = soft_limit(frame[0]);
            let r = soft_limit(frame[1]);
            let off = i * self.channels;
            match self.channels {
                1 => out[off] = (l + r) * 0.5,
                _ => {
                    out[off] = l;
                    out[off + 1] = r;
                    for ch in 2..self.channels {
                        out[off + ch] = 0.0;
                    }
                }
            }
        }

        self.transport.advance(c);
    }

    /// The beat a newly launched track should start on — quantized to the next
    /// bar so added/changed tracks lock to the grid instead of starting wherever
    /// the buffer happened to be.
    fn quantized_start(&self) -> f64 {
        let beat = self.transport.beat;
        if beat <= 0.0 {
            0.0
        } else {
            (beat / 4.0).ceil() * 4.0
        }
    }

    fn rebuild_grouped(&mut self) {
        self.grouped.clear();
        for bus in &self.groups {
            for m in &bus.members {
                self.grouped.insert(m.clone());
            }
        }
    }

    fn apply(&mut self, cmd: Command) {
        match cmd {
            Command::AddTrack { name, build } => {
                let now = self.quantized_start();
                let track = self.build_track(*build, now);
                self.tracks.insert(name, track);
            }
            Command::RemoveTrack(name) => {
                self.tracks.remove(&name);
            }
            Command::StopTrack(name) => {
                if let Some(t) = self.tracks.get_mut(&name) {
                    t.stop();
                }
            }
            Command::SetPatch { track, patch } => {
                if let Some(t) = self.tracks.get_mut(&track) {
                    t.apply_patch(&patch);
                }
            }
            Command::SetMixer {
                track,
                gain,
                pan,
                mute,
            } => {
                if let Some(t) = self.tracks.get_mut(&track) {
                    t.set_mixer(gain, pan, mute);
                }
            }
            Command::SetFx { track, fx, enabled } => {
                if let Some(t) = self.tracks.get_mut(&track) {
                    t.set_fx(fx, enabled);
                }
            }
            Command::SetFxEnabled { track, enabled } => {
                if let Some(t) = self.tracks.get_mut(&track) {
                    t.set_fx_enabled(enabled);
                }
            }
            Command::SetAutomations { track, automations } => {
                if let Some(t) = self.tracks.get_mut(&track) {
                    t.set_automations(automations);
                }
            }
            Command::QueuePattern {
                track,
                func,
                loop_len,
                swing,
            } => {
                if let Some(t) = self.tracks.get_mut(&track) {
                    t.queue_pattern(func, loop_len, swing);
                }
            }
            Command::QueueOneShot { track, notes, swing } => {
                if let Some(t) = self.tracks.get_mut(&track) {
                    t.queue_oneshot(notes, swing);
                }
            }
            Command::SetGroups(groups) => {
                self.groups = groups
                    .into_iter()
                    .map(|g| {
                        let GroupBuild {
                            members,
                            gain,
                            fx,
                            fx_enabled,
                        } = g;
                        let mut bus = Bus::new(gain, self.sample_rate, members);
                        bus.fx = fx;
                        bus.fx_enabled = fx_enabled;
                        bus
                    })
                    .collect();
                self.rebuild_grouped();
            }
            Command::MidiNoteOn { note, vel } => {
                if let Some(name) = &self.midi_track
                    && let Some(t) = self.tracks.get_mut(name)
                {
                    t.note_on(note, vel);
                }
            }
            Command::MidiNoteOff { note } => {
                if let Some(name) = &self.midi_track
                    && let Some(t) = self.tracks.get_mut(name)
                {
                    t.note_off(note);
                }
            }
            Command::SetMidiTrack(name) => {
                self.midi_track = name;
            }
            Command::SetTempo(bpm) => {
                self.transport.bpm = bpm as f64;
            }
        }
    }

    fn build_track(&self, build: TrackBuild, now: f64) -> Track {
        let TrackBuild {
            source,
            patch,
            polyphony,
            gain,
            pan,
            mute,
            fx,
            fx_enabled,
            automations,
            playback,
            seed,
        } = build;

        let mut track = match source {
            BuiltSource::Synth => {
                Track::new_synth(self.sample_rate, polyphony, &patch, gain, pan)
            }
            BuiltSource::Sample(s) => {
                Track::new_sampler(self.sample_rate, polyphony, &patch, gain, pan, s)
            }
        };
        track.mute = mute;
        track.set_seed(seed);
        track.set_fx(fx, fx_enabled);
        track.set_automations(automations);
        match playback {
            Playback::Pattern { func, loop_len, swing } => {
                track.launch_pattern(func, loop_len, now, swing)
            }
            Playback::OneShot { notes, swing } => track.launch_oneshot(notes, now, swing),
            Playback::Silent => {}
        }
        track
    }
}
