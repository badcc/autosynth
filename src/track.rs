use crate::automation::{AutomationFn, Clock, PatternFn, Phrase};
use crate::effects::StereoFrame;
use crate::event::{Event, Param};
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

pub struct Track {
    pub(crate) synth: Synth,
    slot: Option<ClipSlot>,
    pub gain: f32,
    pub(crate) fx_chain: Vec<Box<dyn crate::effects::Effect>>,
    pub(crate) automations: Vec<(Param, AutomationFn)>,
    sample_rate: f32,
    loop_beats: Option<f32>,
    last_automation_tick: u64,
}

impl Track {
    pub fn new(sample_rate: f32, patch: Patch, polyphony: usize) -> Self {
        Self {
            synth: Synth::with_patch(sample_rate, polyphony, patch),
            slot: None,
            gain: 1.0,
            fx_chain: Vec::new(),
            automations: Vec::new(),
            sample_rate,
            loop_beats: None,
            last_automation_tick: u64::MAX,
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

    pub fn set_automations(&mut self, automations: Vec<(Param, AutomationFn)>) {
        self.automations = automations;
        self.last_automation_tick = u64::MAX;
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
                    slot.iteration += 1;
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
            for (param, func) in &mut self.automations {
                self.synth.set_param(*param, func(clock));
            }
        }

        // Render synth (mono) and widen to stereo
        let mono = self.synth.render_sample();
        let mut frame: StereoFrame = [mono, mono];

        // Apply FX chain (stereo)
        for fx in &mut self.fx_chain {
            frame = fx.process(frame, sample_rate);
        }

        [frame[0] * self.gain, frame[1] * self.gain]
    }
}
