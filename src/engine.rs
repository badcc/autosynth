use std::sync::mpsc;

use crate::clip::Clip;
use crate::event::Param;
use crate::patch::Patch;
use crate::score::Tempo;
use crate::session::Session;

pub enum Command {
    // Track management
    AddTrack {
        name: String,
        patch: Patch,
        polyphony: usize,
    },
    RemoveTrack(String),

    // Clip operations (track-scoped)
    LaunchClip {
        track: String,
        clip: Clip,
    },
    StopClip {
        track: String,
        clip: String,
    },
    StopAll,

    // Patch/param (track-scoped)
    SetPatch {
        track: String,
        patch: Patch,
    },
    SetParam {
        track: String,
        param: Param,
        value: f32,
    },

    // Effects (track-scoped)
    AddEffect {
        track: String,
        effect: Box<dyn crate::effects::Effect>,
    },
    ClearEffects(String),

    // Global
    SetTempo(Tempo),
}

#[derive(Clone)]
pub struct EngineHandle {
    tx: mpsc::Sender<Command>,
}

impl EngineHandle {
    pub fn add_track(&self, name: &str, patch: Patch) {
        let _ = self.tx.send(Command::AddTrack {
            name: name.to_string(),
            patch,
            polyphony: 8,
        });
    }

    pub fn add_track_with_polyphony(&self, name: &str, patch: Patch, polyphony: usize) {
        let _ = self.tx.send(Command::AddTrack {
            name: name.to_string(),
            patch,
            polyphony,
        });
    }

    pub fn remove_track(&self, name: &str) {
        let _ = self.tx.send(Command::RemoveTrack(name.to_string()));
    }

    pub fn launch(&self, track: &str, clip: Clip) {
        let _ = self.tx.send(Command::LaunchClip {
            track: track.to_string(),
            clip,
        });
    }

    pub fn stop(&self, track: &str, clip: &str) {
        let _ = self.tx.send(Command::StopClip {
            track: track.to_string(),
            clip: clip.to_string(),
        });
    }

    pub fn stop_all(&self) {
        let _ = self.tx.send(Command::StopAll);
    }

    pub fn set_patch(&self, track: &str, patch: Patch) {
        let _ = self.tx.send(Command::SetPatch {
            track: track.to_string(),
            patch,
        });
    }

    pub fn set_param(&self, track: &str, param: Param, value: f32) {
        let _ = self.tx.send(Command::SetParam {
            track: track.to_string(),
            param,
            value,
        });
    }

    pub fn add_effect(&self, track: &str, effect: impl crate::effects::Effect + 'static) {
        let _ = self.tx.send(Command::AddEffect {
            track: track.to_string(),
            effect: Box::new(effect),
        });
    }

    pub fn clear_effects(&self, track: &str) {
        let _ = self.tx.send(Command::ClearEffects(track.to_string()));
    }

    pub fn set_tempo(&self, tempo: Tempo) {
        let _ = self.tx.send(Command::SetTempo(tempo));
    }
}

pub struct Engine {
    session: Session,
    rx: mpsc::Receiver<Command>,
    sample_pos: u64,
    channels: usize,
}

impl Engine {
    pub fn new(
        sample_rate: f32,
        channels: usize,
        tempo: Tempo,
    ) -> (Self, EngineHandle) {
        let (tx, rx) = mpsc::channel();
        let engine = Self {
            session: Session::new(tempo, sample_rate),
            rx,
            sample_pos: 0,
            channels,
        };
        let handle = EngineHandle { tx };
        (engine, handle)
    }

    pub fn render(&mut self, output: &mut [f32]) {
        while let Ok(cmd) = self.rx.try_recv() {
            self.apply_command(cmd);
        }

        let start = self.sample_pos;
        let frames = output.len() / self.channels;
        self.session.render(output, self.channels, start);
        self.sample_pos += frames as u64;
    }

    /// Build a cpal output stream, consuming the engine.
    ///
    /// The engine moves into the audio callback. Use the `EngineHandle`
    /// returned from `Engine::new` to send commands (launch clips, set
    /// params, etc.) from any thread.
    pub fn build_stream(
        self,
        device: &cpal::Device,
        config: &cpal::StreamConfig,
    ) -> Result<cpal::Stream, cpal::BuildStreamError> {
        let mut engine = self;

        use cpal::traits::DeviceTrait;
        device.build_output_stream(
            config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                engine.render(data);
            },
            |err| eprintln!("audio stream error: {err}"),
            None,
        )
    }

    fn apply_command(&mut self, cmd: Command) {
        match cmd {
            Command::AddTrack {
                name,
                patch,
                polyphony,
            } => {
                self.session.add_track(name, patch, polyphony);
            }
            Command::RemoveTrack(name) => {
                self.session.remove_track(&name);
            }
            Command::LaunchClip { track, clip } => {
                self.session.launch(&track, clip, self.sample_pos);
            }
            Command::StopClip { track, clip } => {
                self.session.stop(&track, &clip);
            }
            Command::StopAll => {
                self.session.stop_all();
            }
            Command::SetPatch { track, patch } => {
                if let Some(t) = self.session.track_mut(&track) {
                    t.synth.apply_patch(patch);
                }
            }
            Command::SetParam {
                track,
                param,
                value,
            } => {
                if let Some(t) = self.session.track_mut(&track) {
                    t.synth.set_param(param, value);
                }
            }
            Command::AddEffect { track, effect } => {
                if let Some(t) = self.session.track_mut(&track) {
                    t.fx_chain.push(effect);
                }
            }
            Command::ClearEffects(track) => {
                if let Some(t) = self.session.track_mut(&track) {
                    t.fx_chain.clear();
                }
            }
            Command::SetTempo(tempo) => {
                self.session.set_tempo(tempo);
            }
        }
    }
}
