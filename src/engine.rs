use std::sync::mpsc;

use crate::automation::{Automation, PatternFn};
use crate::event::SynthParam;
use crate::patch::Patch;
use crate::score::{Score, Tempo};
use crate::session::Session;
use crate::track::PendingUpdate;

pub(crate) enum Command {
    // Track management
    AddTrack {
        name: String,
        patch: Patch,
        polyphony: usize,
    },
    RemoveTrack(String),

    // Playback
    Launch {
        track: String,
        score: Score,
        loop_beats: Option<f32>,
        pattern_fn: Option<PatternFn>,
    },
    StopTrack(String),
    StopAll,

    // Patch/param (track-scoped)
    SetPatch {
        track: String,
        patch: Patch,
    },
    SetParam {
        track: String,
        param: SynthParam,
        value: f32,
    },

    // Automations (track-scoped)
    SetAutomations {
        track: String,
        automations: Vec<Automation>,
    },

    // Effects (track-scoped)
    AddEffect {
        track: String,
        effect: Box<dyn crate::effects::Effect>,
    },
    ClearEffects(String),
    SetFxEnabled {
        track: String,
        enabled: Vec<bool>,
    },

    // Hot-reload: boundary-aligned update for existing tracks
    UpdateTrack {
        track: String,
        patch: Patch,
        /// None = keep existing effects (preserves delay buffers etc.)
        effects: Option<(Vec<Box<dyn crate::effects::Effect>>, Vec<bool>)>,
        automations: Vec<Automation>,
        pattern_fn: Option<PatternFn>,
        loop_beats: Option<f32>,
    },

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

    pub fn launch(&self, track: &str, score: Score) {
        let _ = self.tx.send(Command::Launch {
            track: track.to_string(),
            score,
            loop_beats: None,
            pattern_fn: None,
        });
    }

    pub fn launch_with_pattern(&self, track: &str, loop_beats: f32, pattern_fn: PatternFn) {
        let _ = self.tx.send(Command::Launch {
            track: track.to_string(),
            score: Score::new(),
            loop_beats: Some(loop_beats),
            pattern_fn: Some(pattern_fn),
        });
    }

    pub fn stop(&self, track: &str) {
        let _ = self.tx.send(Command::StopTrack(track.to_string()));
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

    pub fn set_param(&self, track: &str, param: SynthParam, value: f32) {
        let _ = self.tx.send(Command::SetParam {
            track: track.to_string(),
            param,
            value,
        });
    }

    pub(crate) fn set_automations(&self, track: &str, automations: Vec<Automation>) {
        let _ = self.tx.send(Command::SetAutomations {
            track: track.to_string(),
            automations,
        });
    }

    pub fn add_effect(&self, track: &str, effect: impl crate::effects::Effect + 'static) {
        let _ = self.tx.send(Command::AddEffect {
            track: track.to_string(),
            effect: Box::new(effect),
        });
    }

    pub fn add_effect_boxed(&self, track: &str, effect: Box<dyn crate::effects::Effect>) {
        let _ = self.tx.send(Command::AddEffect {
            track: track.to_string(),
            effect,
        });
    }

    pub fn clear_effects(&self, track: &str) {
        let _ = self.tx.send(Command::ClearEffects(track.to_string()));
    }

    pub(crate) fn set_fx_enabled(&self, track: &str, enabled: Vec<bool>) {
        let _ = self.tx.send(Command::SetFxEnabled {
            track: track.to_string(),
            enabled,
        });
    }

    pub(crate) fn update_track(
        &self,
        track: &str,
        patch: Patch,
        effects: Option<(Vec<Box<dyn crate::effects::Effect>>, Vec<bool>)>,
        automations: Vec<Automation>,
        pattern_fn: Option<PatternFn>,
        loop_beats: Option<f32>,
    ) {
        let _ = self.tx.send(Command::UpdateTrack {
            track: track.to_string(),
            patch,
            effects,
            automations,
            pattern_fn,
            loop_beats,
        });
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
    /// returned from `Engine::new` to send commands from any thread.
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
            Command::Launch {
                track,
                score,
                loop_beats,
                pattern_fn,
            } => {
                self.session
                    .launch(&track, score, loop_beats, self.sample_pos, pattern_fn);
            }
            Command::StopTrack(track) => {
                self.session.stop(&track);
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
            Command::SetAutomations {
                track,
                automations,
            } => {
                if let Some(t) = self.session.track_mut(&track) {
                    t.set_automations(automations);
                }
            }
            Command::AddEffect { track, effect } => {
                if let Some(t) = self.session.track_mut(&track) {
                    t.fx_chain.push(effect);
                    t.fx_enabled.push(true);
                }
            }
            Command::ClearEffects(track) => {
                if let Some(t) = self.session.track_mut(&track) {
                    t.fx_chain.clear();
                    t.fx_enabled.clear();
                }
            }
            Command::SetFxEnabled { track, enabled } => {
                if let Some(t) = self.session.track_mut(&track) {
                    t.fx_enabled = enabled;
                }
            }
            Command::UpdateTrack {
                track,
                patch,
                effects,
                automations,
                pattern_fn,
                loop_beats,
            } => {
                let tempo = self.session.tempo();
                if let Some(t) = self.session.track_mut(&track) {
                    let update = PendingUpdate {
                        patch,
                        effects,
                        automations,
                        pattern_fn,
                        loop_beats,
                    };
                    if t.has_active_loop() {
                        eprintln!("[engine] queued pending update for '{track}' (has active loop)");
                        t.pending = Some(update);
                    } else {
                        eprintln!("[engine] applying update immediately for '{track}' (no active loop)");
                        t.apply_update(update, tempo);
                    }
                } else {
                    eprintln!("[engine] UpdateTrack: track '{track}' not found in session!");
                }
            }
            Command::SetTempo(tempo) => {
                self.session.set_tempo(tempo);
            }
        }
    }
}
