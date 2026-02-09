use std::collections::HashMap;

use crate::clip::Clip;
use crate::patch::Patch;
use crate::score::{SequencePlayer, Tempo};
use crate::synth::Synth;

struct ClipSlot {
    player: SequencePlayer,
    start_sample: u64,
    active: bool,
}

pub struct Track {
    pub(crate) synth: Synth,
    slots: HashMap<String, ClipSlot>,
    pub gain: f32,
    pub(crate) fx_chain: Vec<Box<dyn crate::effects::Effect>>,
}

impl Track {
    pub fn new(sample_rate: f32, patch: Patch, polyphony: usize) -> Self {
        Self {
            synth: Synth::with_patch(sample_rate, polyphony, patch),
            slots: HashMap::new(),
            gain: 1.0,
            fx_chain: Vec::new(),
        }
    }

    pub fn launch(&mut self, clip: Clip, tempo: Tempo, sample_rate: f32, current_sample: u64) {
        let name = clip.name.clone();
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

        self.slots.insert(
            name,
            ClipSlot {
                player,
                start_sample: current_sample,
                active: true,
            },
        );
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

    /// Render one sample for this track: dispatch events, render synth, apply FX chain, apply gain.
    pub fn render_sample(&mut self, sample_idx: u64, sample_rate: f32) -> f32 {
        // Dispatch clip events
        for slot in self.slots.values_mut() {
            if !slot.active || sample_idx < slot.start_sample {
                continue;
            }

            slot.player.advance_loop(sample_idx);

            while let Some(e) = slot.player.peek() {
                if e.sample > sample_idx {
                    break;
                }
                slot.player.pop();
                self.synth.apply_event(e.kind);
            }
        }

        // Render synth
        let mut sample = self.synth.render_sample();

        // Apply FX chain
        for fx in &mut self.fx_chain {
            sample = fx.process(sample, sample_rate);
        }

        sample * self.gain
    }
}
