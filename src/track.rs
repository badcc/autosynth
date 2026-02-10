use crate::automation::{AutoCmd, Automation, Clock, PatternFn, Phrase};
use crate::effects::StereoFrame;
use crate::event::Event;
use crate::patch::Patch;
use crate::score::{time_to_sample, Score, SequencePlayer, Tempo};
use crate::synth::Synth;

struct ClipGenerator {
    func: PatternFn,
    tempo: Tempo,
    sample_rate: f32,
}

impl ClipGenerator {
    fn regenerate(&mut self, player: &mut SequencePlayer, beat: f32, iteration: u32) {
        let mut phrase = Phrase::with_context(beat, iteration);
        (self.func)(&mut phrase);
        let mut events: Vec<Event> = phrase
            .into_events()
            .into_iter()
            .map(|(t, k)| Event {
                sample: time_to_sample(t, self.tempo, self.sample_rate),
                kind: k,
            })
            .collect();
        events.sort_by_key(|e| e.sample);
        player.replace_events(events);
    }
}

struct ClipSlot {
    player: SequencePlayer,
    generator: Option<ClipGenerator>,
    start_sample: u64,
    active: bool,
    iteration: u32,
}

/// Buffered update applied at the next loop boundary.
pub(crate) struct PendingUpdate {
    pub patch: Patch,
    /// None = keep existing effects chain (preserves delay buffers etc.)
    pub effects: Option<(Vec<Box<dyn crate::effects::Effect>>, Vec<bool>)>,
    pub automations: Vec<Automation>,
    pub pattern_fn: Option<PatternFn>,
    pub loop_beats: Option<f32>,
}

pub struct Track {
    pub(crate) synth: Synth,
    slot: Option<ClipSlot>,
    pub gain: f32,
    pub(crate) fx_chain: Vec<Box<dyn crate::effects::Effect>>,
    pub(crate) fx_enabled: Vec<bool>,
    pub(crate) automations: Vec<Automation>,
    sample_rate: f32,
    loop_beats: Option<f32>,
    last_automation_tick: u64,
    pub(crate) pending: Option<PendingUpdate>,
}

impl Track {
    pub fn new(sample_rate: f32, patch: Patch, polyphony: usize) -> Self {
        Self {
            synth: Synth::with_patch(sample_rate, polyphony, patch),
            slot: None,
            gain: 1.0,
            fx_chain: Vec::new(),
            fx_enabled: Vec::new(),
            automations: Vec::new(),
            sample_rate,
            loop_beats: None,
            last_automation_tick: u64::MAX,
            pending: None,
        }
    }

    pub fn launch(
        &mut self,
        score: Score,
        loop_beats: Option<f32>,
        tempo: Tempo,
        current_sample: u64,
        pattern_fn: Option<PatternFn>,
    ) {
        self.loop_beats = loop_beats;
        let sequence = score.to_sequence(tempo, self.sample_rate);

        let player = if let Some(beats) = loop_beats {
            let seconds = beats * 60.0 / tempo.bpm;
            let loop_samples = (seconds * self.sample_rate) as u64;
            sequence.loop_player(loop_samples)
        } else {
            sequence.player()
        };

        let generator = pattern_fn.map(|func| ClipGenerator {
            func,
            tempo,
            sample_rate: self.sample_rate,
        });

        let mut slot = ClipSlot {
            player,
            generator,
            start_sample: current_sample,
            active: true,
            iteration: 0,
        };

        if let Some(ref mut g) = slot.generator {
            let beat = current_sample as f64 / self.sample_rate as f64 * tempo.bpm as f64 / 60.0;
            g.regenerate(&mut slot.player, beat as f32, 0);
        }

        self.slot = Some(slot);
    }

    pub fn stop(&mut self) {
        if let Some(slot) = &mut self.slot {
            slot.active = false;
        }
    }

    pub(crate) fn set_automations(&mut self, automations: Vec<Automation>) {
        self.automations = automations;
        self.last_automation_tick = u64::MAX;
    }

    /// Whether this track has an active looping clip (i.e. a loop boundary will come).
    pub(crate) fn has_active_loop(&self) -> bool {
        self.slot
            .as_ref()
            .is_some_and(|s| s.active && s.player.loop_len.is_some())
    }

    /// Apply a pending update immediately (used for non-looping tracks).
    pub(crate) fn apply_update(&mut self, update: PendingUpdate, tempo: Tempo) {
        self.synth.apply_patch(update.patch);
        if let Some((effects, fx_enabled)) = update.effects {
            self.fx_chain = effects;
            self.fx_enabled = fx_enabled;
        }
        self.automations = update.automations;
        self.last_automation_tick = u64::MAX;

        if let Some(pf) = update.pattern_fn {
            if let Some(slot) = &mut self.slot {
                slot.generator = Some(ClipGenerator {
                    func: pf,
                    tempo,
                    sample_rate: self.sample_rate,
                });
            }
        }
        if let Some(beats) = update.loop_beats {
            self.loop_beats = Some(beats);
            if let Some(slot) = &mut self.slot {
                let seconds = beats * 60.0 / tempo.bpm;
                let loop_samples = (seconds * self.sample_rate) as u64;
                slot.player.set_loop_len(loop_samples);
            }
        }
    }

    /// Render one stereo frame for this track: dispatch events, render synth (mono),
    /// widen to stereo, apply FX chain, apply gain.
    pub fn render_sample(&mut self, sample_idx: u64, tempo: Tempo) -> StereoFrame {
        let sample_rate = self.sample_rate;

        // Dispatch clip events
        if let Some(slot) = &mut self.slot {
            if slot.active && sample_idx >= slot.start_sample {
                let looped = slot.player.advance_loop(sample_idx);

                if looped {
                    self.synth.all_notes_off();
                    slot.iteration += 1;

                    // Apply pending hot-reload update at loop boundary
                    if let Some(update) = self.pending.take() {
                        eprintln!("[track] applying pending update at loop boundary (iteration {})", slot.iteration);
                        self.synth.apply_patch(update.patch);
                        if let Some((effects, fx_enabled)) = update.effects {
                            self.fx_chain = effects;
                            self.fx_enabled = fx_enabled;
                        }
                        self.automations = update.automations;
                        self.last_automation_tick = u64::MAX;

                        if let Some(pf) = update.pattern_fn {
                            slot.generator = Some(ClipGenerator {
                                func: pf,
                                tempo,
                                sample_rate,
                            });
                        }
                        if let Some(beats) = update.loop_beats {
                            self.loop_beats = Some(beats);
                            let seconds = beats * 60.0 / tempo.bpm;
                            let loop_samples = (seconds * sample_rate) as u64;
                            slot.player.set_loop_len(loop_samples);
                        }
                    }

                    if let Some(ref mut g) = slot.generator {
                        let beat =
                            sample_idx as f64 / sample_rate as f64 * tempo.bpm as f64 / 60.0;
                        g.regenerate(&mut slot.player, beat as f32, slot.iteration);
                    }
                }

                while let Some(e) = slot.player.peek() {
                    if e.sample > sample_idx {
                        break;
                    }
                    slot.player.pop();
                    self.synth.apply_event(e.kind);
                }
            }
        }

        // Apply automations (once per 16th note)
        let beat = sample_idx as f64 / sample_rate as f64 * tempo.bpm as f64 / 60.0;
        let tick = (beat * 4.0) as u64;
        if tick != self.last_automation_tick {
            self.last_automation_tick = tick;
            let beat_f32 = beat as f32;
            let (local, iteration) = match self.loop_beats {
                Some(len) => (beat_f32 % len, (beat_f32 / len) as u32),
                None => (beat_f32, 0),
            };
            let clock = Clock {
                beat: beat_f32,
                local,
                iteration,
            };
            for auto in &mut self.automations {
                match auto(clock) {
                    AutoCmd::Synth(param, value) => {
                        self.synth.set_param(param, value);
                    }
                    AutoCmd::FilterType(ft) => {
                        self.synth.set_filter_type(ft);
                    }
                    AutoCmd::Retrigger(mode) => {
                        self.synth.set_retrigger(mode);
                    }
                    AutoCmd::OscParam { osc_index, param, value } => {
                        self.synth.set_osc_param(osc_index, param, value);
                    }
                    AutoCmd::OscWaveform { osc_index, waveform } => {
                        self.synth.set_osc_waveform(osc_index, waveform);
                    }
                    AutoCmd::FxParam { fx_index, slot, value } => {
                        if let Some(fx) = self.fx_chain.get_mut(fx_index) {
                            fx.set_param(slot, value);
                        }
                    }
                    AutoCmd::FxEnabled { fx_index, enabled } => {
                        if let Some(e) = self.fx_enabled.get_mut(fx_index) {
                            *e = enabled;
                        }
                    }
                }
            }
        }

        // Render synth (mono) and widen to stereo
        let mono = self.synth.render_sample();
        let mut frame: StereoFrame = [mono, mono];

        // Apply FX chain (stereo), skipping disabled effects
        for (i, fx) in self.fx_chain.iter_mut().enumerate() {
            if self.fx_enabled.get(i).copied().unwrap_or(true) {
                frame = fx.process(frame, sample_rate);
            }
        }

        [frame[0] * self.gain, frame[1] * self.gain]
    }
}
