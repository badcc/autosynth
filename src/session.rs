use std::collections::HashMap;

use crate::clip::Clip;
use crate::patch::Patch;
use crate::score::Tempo;
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

    pub fn remove_track(&mut self, name: &str) {
        self.tracks.remove(name);
    }

    pub fn track_mut(&mut self, name: &str) -> Option<&mut Track> {
        self.tracks.get_mut(name)
    }

    pub fn launch(&mut self, track: &str, clip: Clip, current_sample: u64) {
        // Auto-create track with default patch if it doesn't exist
        if !self.tracks.contains_key(track) {
            self.add_track(track.to_string(), Patch::new(), 8);
        }
        if let Some(t) = self.tracks.get_mut(track) {
            t.launch(clip, self.tempo, self.sample_rate, current_sample);
        }
    }

    pub fn stop(&mut self, track: &str, clip: &str) {
        if let Some(t) = self.tracks.get_mut(track) {
            t.stop(clip);
        }
    }

    pub fn stop_all(&mut self) {
        for track in self.tracks.values_mut() {
            track.stop_all();
        }
    }

    /// Render all tracks into the output buffer, mixing them together.
    pub fn render(&mut self, output: &mut [f32], channels: usize, start_sample: u64) {
        let frames = output.len() / channels;
        let sample_rate = self.sample_rate;

        for frame in 0..frames {
            let sample_idx = start_sample + frame as u64;

            let mut mix = 0.0;
            for track in self.tracks.values_mut() {
                mix += track.render_sample(sample_idx, sample_rate);
            }

            let clamped = mix.clamp(-1.0, 1.0);
            for ch in 0..channels {
                output[frame * channels + ch] = clamped;
            }
        }
    }
}
