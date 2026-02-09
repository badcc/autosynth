use std::collections::HashMap;

use crate::automation::{AutomationFn, Clock, PatternFn, Phrase};
use crate::clip::Clip;
use crate::effects::StereoFrame;
use crate::event::{Event, Param};
use crate::patch::Patch;
use crate::score::{time_to_sample, SequencePlayer, Tempo};
use crate::synth::Synth;

struct ClipGenerator {
    func: PatternFn,
    tempo: Tempo,
    sample_rate: f32,
}

impl ClipGenerator {
    /// Run the pattern function to produce fresh events and replace player events.
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
    slots: HashMap<String, ClipSlot>,
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
            slots: HashMap::new(),
            gain: 1.0,
            fx_chain: Vec::new(),
            automations: Vec::new(),
            sample_rate,
            loop_beats: None,
            last_automation_tick: u64::MAX,
        }
    }

    pub fn launch(&mut self, clip: Clip, tempo: Tempo, sample_rate: f32, current_sample: u64) {
        self.launch_inner(clip, tempo, sample_rate, current_sample, None);
    }

    pub fn launch_with_pattern(
        &mut self,
        clip: Clip,
        tempo: Tempo,
        sample_rate: f32,
        current_sample: u64,
        pattern_fn: PatternFn,
    ) {
        self.launch_inner(clip, tempo, sample_rate, current_sample, Some(pattern_fn));
    }

    fn launch_inner(
        &mut self,
        clip: Clip,
        tempo: Tempo,
        sample_rate: f32,
        current_sample: u64,
        pattern_fn: Option<PatternFn>,
    ) {
        let name = clip.name.clone();
        self.loop_beats = clip.loop_beats;
        let sequence = clip.score.to_sequence(tempo, sample_rate);

        let loop_samples = clip.loop_beats.map(|beats| {
            let seconds = beats * 60.0 / tempo.bpm;
            (seconds * sample_rate) as u64
        });

        let player = if let Some(loop_len) = loop_samples {
            sequence.loop_player(loop_len)
        } else {
            sequence.player()
        };

        let generator = pattern_fn.map(|func| ClipGenerator {
            func,
            tempo,
            sample_rate,
        });

        let mut slot = ClipSlot {
            player,
            generator,
            start_sample: current_sample,
            active: true,
            iteration: 0,
        };

        // Run generator immediately for first iteration
        if let Some(ref mut g) = slot.generator {
            let beat = current_sample as f64 / sample_rate as f64 * tempo.bpm as f64 / 60.0;
            g.regenerate(&mut slot.player, beat as f32, 0);
        }

        self.slots.insert(name, slot);
    }

    pub fn stop(&mut self, clip_name: &str) {
        if let Some(slot) = self.slots.get_mut(clip_name) {
            slot.active = false;
        }
    }

    pub fn stop_all(&mut self) {
        for slot in self.slots.values_mut() {
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
        for slot in self.slots.values_mut() {
            if !slot.active || sample_idx < slot.start_sample {
                continue;
            }

            let looped = slot.player.advance_loop(sample_idx);

            // Regenerate on loop boundary
            if looped {
                slot.iteration += 1;
                if let Some(ref mut g) = slot.generator {
                    let beat = sample_idx as f64 / sample_rate as f64 * tempo.bpm as f64 / 60.0;
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

        // Apply gain to both channels
        [frame[0] * self.gain, frame[1] * self.gain]
    }
}
