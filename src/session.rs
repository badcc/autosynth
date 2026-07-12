use std::collections::HashMap;
use std::sync::Arc;

use crate::automation::PatternFn;
use crate::patch::Patch;
use crate::sample::SampleData;
use crate::score::{Score, Tempo};
use crate::track::Track;

pub struct Session {
    tracks: HashMap<String, Track>,
    tempo: Tempo,
    sample_rate: f32,
}

impl Session {
    pub fn new(tempo: Tempo, sample_rate: f32) -> Self {
        Self {
            tracks: HashMap::new(),
            tempo,
            sample_rate,
        }
    }

    pub fn tempo(&self) -> Tempo {
        self.tempo
    }

    pub fn set_tempo(&mut self, tempo: Tempo) {
        self.tempo = tempo;
    }

    pub fn add_track(&mut self, name: String, patch: Patch, polyphony: usize) {
        self.tracks
            .insert(name, Track::new(self.sample_rate, patch, polyphony));
    }

    pub fn add_sampler_track(
        &mut self,
        name: String,
        patch: Patch,
        polyphony: usize,
        data: Arc<SampleData>,
        root_note: u8,
    ) {
        self.tracks.insert(
            name,
            Track::new_sampler(self.sample_rate, patch, polyphony, data, root_note),
        );
    }

    pub fn add_kit_track(
        &mut self,
        name: String,
        patch: Patch,
        polyphony: usize,
        map: HashMap<u8, Arc<SampleData>>,
    ) {
        self.tracks.insert(
            name,
            Track::new_kit(self.sample_rate, patch, polyphony, map),
        );
    }

    pub fn remove_track(&mut self, name: &str) {
        self.tracks.remove(name);
    }

    pub fn track_mut(&mut self, name: &str) -> Option<&mut Track> {
        self.tracks.get_mut(name)
    }

    pub fn launch(
        &mut self,
        track: &str,
        score: Score,
        loop_beats: Option<f32>,
        current_sample: u64,
        pattern_fn: Option<PatternFn>,
    ) {
        if !self.tracks.contains_key(track) {
            self.add_track(track.to_string(), Patch::new(), 8);
        }
        if let Some(t) = self.tracks.get_mut(track) {
            t.launch(score, loop_beats, self.tempo, current_sample, pattern_fn);
        }
    }

    pub fn stop(&mut self, track: &str) {
        if let Some(t) = self.tracks.get_mut(track) {
            t.stop();
        }
    }

    pub fn stop_all(&mut self) {
        for track in self.tracks.values_mut() {
            track.stop();
        }
    }

    /// Render all tracks into the output buffer, mixing them together.
    pub fn render(&mut self, output: &mut [f32], channels: usize, start_sample: u64) {
        let frames = output.len() / channels;
        let tempo = self.tempo;

        for frame in 0..frames {
            let sample_idx = start_sample + frame as u64;

            let mut mix_l = 0.0_f32;
            let mut mix_r = 0.0_f32;
            for track in self.tracks.values_mut() {
                let [l, r] = track.render_sample(sample_idx, tempo);
                mix_l += l;
                mix_r += r;
            }

            let offset = frame * channels;
            match channels {
                1 => {
                    output[offset] = ((mix_l + mix_r) * 0.5).clamp(-1.0, 1.0);
                }
                2 => {
                    output[offset] = mix_l.clamp(-1.0, 1.0);
                    output[offset + 1] = mix_r.clamp(-1.0, 1.0);
                }
                _ => {
                    // First two channels get L/R, rest silence
                    output[offset] = mix_l.clamp(-1.0, 1.0);
                    output[offset + 1] = mix_r.clamp(-1.0, 1.0);
                    for ch in 2..channels {
                        output[offset + ch] = 0.0;
                    }
                }
            }
        }
    }
}
