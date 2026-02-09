use crate::patch::Patch;
use crate::pattern::Pattern;
use crate::score::{Score, Time};

#[derive(Clone, Debug)]
pub struct Clip {
    pub name: String,
    pub score: Score,
    pub loop_beats: Option<f32>,
}

impl Clip {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            score: Score::new(),
            loop_beats: None,
        }
    }

    pub fn looped(name: impl Into<String>, beats: f32) -> Self {
        Self {
            name: name.into(),
            score: Score::new(),
            loop_beats: Some(beats),
        }
    }

    pub fn oneshot(name: impl Into<String>) -> Self {
        Self::new(name)
    }

    pub fn build<F>(mut self, f: F) -> Self
    where
        F: FnOnce(&mut Score),
    {
        f(&mut self.score);
        self
    }

    pub fn with_patch(mut self, patch: Patch) -> Self {
        self.score.patch(Time::Beats(0.0), patch);
        self
    }

    pub fn with_pattern(mut self, start_beat: f32, pattern: &Pattern) -> Self {
        self.score.pattern(Time::Beats(start_beat), pattern);
        self
    }

    pub fn from_pattern(name: impl Into<String>, pattern: &Pattern, loop_beats: f32) -> Self {
        let mut score = Score::new();
        score.pattern(Time::Beats(0.0), pattern);
        Self {
            name: name.into(),
            score,
            loop_beats: Some(loop_beats),
        }
    }

    pub fn is_looping(&self) -> bool {
        self.loop_beats.is_some()
    }
}
