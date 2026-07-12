use std::collections::HashMap;
use std::sync::Arc;

use tracing::debug;

use crate::automation::{AutoCmd, Automation, Clock, OscParam, PatternFn, Phrase};
use crate::effects::StereoFrame;
use crate::envelope::RetriggerMode;
use crate::event::{Event, EventKind, SynthParam};
use crate::filter::FilterType;
use crate::patch::Patch;
use crate::sample::SampleData;
use crate::sampler::Sampler;
use crate::score::{time_to_sample, Score, SequencePlayer, Tempo};
use crate::synth::Synth;
use crate::waveform::Waveform;

// ── SoundSource: enum over Synth/Sampler to avoid dyn dispatch ──

pub(crate) enum SoundSource {
    Synth(Synth),
    Sampler(Sampler),
}

impl SoundSource {
    pub fn apply_event(&mut self, event: EventKind) {
        match self {
            Self::Synth(s) => s.apply_event(event),
            Self::Sampler(s) => s.apply_event(event),
        }
    }

    pub fn render_sample(&mut self) -> f32 {
        match self {
            Self::Synth(s) => s.render_sample(),
            Self::Sampler(s) => s.render_sample(),
        }
    }

    pub fn all_notes_off(&mut self) {
        match self {
            Self::Synth(s) => s.all_notes_off(),
            Self::Sampler(s) => s.all_notes_off(),
        }
    }

    pub fn apply_patch(&mut self, patch: Patch) {
        match self {
            Self::Synth(s) => s.apply_patch(patch),
            Self::Sampler(s) => s.apply_patch(patch),
        }
    }

    pub fn set_param(&mut self, param: SynthParam, value: f32) {
        match self {
            Self::Synth(s) => s.set_param(param, value),
            Self::Sampler(s) => s.set_param(param, value),
        }
    }

    pub fn set_filter_type(&mut self, ft: FilterType) {
        match self {
            Self::Synth(s) => s.set_filter_type(ft),
            Self::Sampler(s) => s.set_filter_type(ft),
        }
    }

    pub fn set_retrigger(&mut self, mode: RetriggerMode) {
        match self {
            Self::Synth(s) => s.set_retrigger(mode),
            Self::Sampler(s) => s.set_retrigger(mode),
        }
    }

    pub fn set_osc_param(&mut self, index: usize, param: OscParam, value: f32) {
        match self {
            Self::Synth(s) => s.set_osc_param(index, param, value),
            Self::Sampler(s) => s.set_osc_param(index, param, value),
        }
    }

    pub fn set_osc_waveform(&mut self, index: usize, waveform: Waveform) {
        match self {
            Self::Synth(s) => s.set_osc_waveform(index, waveform),
            Self::Sampler(s) => s.set_osc_waveform(index, waveform),
        }
    }

    pub fn note_on(&mut self, note: u8, vel: f32) {
        match self {
            Self::Synth(s) => s.note_on(note, vel),
            Self::Sampler(s) => s.note_on(note, vel),
        }
    }

    pub fn note_off(&mut self, note: u8) {
        match self {
            Self::Synth(s) => s.note_off(note),
            Self::Sampler(s) => s.note_off(note),
        }
    }
}

// ── ClipGenerator / ClipSlot (unchanged) ──

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

/// Timing update queued to the next loop boundary (pattern + loop length).
/// Sound changes (patch, effects, automations) are applied immediately.
pub(crate) struct PendingTimingUpdate {
    pub pattern_fn: Option<PatternFn>,
    pub loop_beats: Option<f32>,
}

pub struct Track {
    pub(crate) source: SoundSource,
    slot: Option<ClipSlot>,
    pub gain: f32,
    pub(crate) fx_chain: Vec<Box<dyn crate::effects::Effect>>,
    pub(crate) fx_enabled: Vec<bool>,
    pub(crate) automations: Vec<Automation>,
    sample_rate: f32,
    loop_beats: Option<f32>,
    last_automation_tick: u64,
    pub(crate) pending: Option<PendingTimingUpdate>,
}

impl Track {
    pub fn new(sample_rate: f32, patch: Patch, polyphony: usize) -> Self {
        Self {
            source: SoundSource::Synth(Synth::with_patch(sample_rate, polyphony, patch)),
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

    pub fn new_sampler(
        sample_rate: f32,
        patch: Patch,
        polyphony: usize,
        data: Arc<SampleData>,
        root_note: u8,
    ) -> Self {
        Self {
            source: SoundSource::Sampler(Sampler::new(
                sample_rate, polyphony, patch, data, root_note,
            )),
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

    pub fn new_kit(
        sample_rate: f32,
        patch: Patch,
        polyphony: usize,
        map: HashMap<u8, Arc<SampleData>>,
    ) -> Self {
        Self {
            source: SoundSource::Sampler(Sampler::new_kit(sample_rate, polyphony, patch, map)),
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

    /// Apply sound changes (patch, effects, automations) immediately.
    pub(crate) fn apply_sound_update(
        &mut self,
        patch: Patch,
        effects: Option<(Vec<Box<dyn crate::effects::Effect>>, Vec<bool>)>,
        automations: Vec<Automation>,
    ) {
        self.source.apply_patch(patch);
        if let Some((fx, fx_enabled)) = effects {
            self.fx_chain = fx;
            self.fx_enabled = fx_enabled;
        }
        self.automations = automations;
        self.last_automation_tick = u64::MAX;
    }

    /// Apply timing changes (pattern_fn, loop_beats) immediately.
    /// Used for non-looping tracks where there's no boundary to wait for.
    pub(crate) fn apply_timing_update(&mut self, update: PendingTimingUpdate, tempo: Tempo) {
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

    /// Render one stereo frame for this track: dispatch events, render source (mono),
    /// widen to stereo, apply FX chain, apply gain.
    pub fn render_sample(&mut self, sample_idx: u64, tempo: Tempo) -> StereoFrame {
        let sample_rate = self.sample_rate;

        // Dispatch clip events
        if let Some(slot) = &mut self.slot {
            if slot.active && sample_idx >= slot.start_sample {
                let looped = slot.player.advance_loop(sample_idx);

                if looped {
                    self.source.all_notes_off();
                    slot.iteration += 1;

                    // Apply pending timing update at loop boundary
                    if let Some(update) = self.pending.take() {
                        debug!(iteration = slot.iteration, "applying timing update at loop boundary");
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
                    self.source.apply_event(e.kind);
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
                        self.source.set_param(param, value);
                    }
                    AutoCmd::FilterType(ft) => {
                        self.source.set_filter_type(ft);
                    }
                    AutoCmd::Retrigger(mode) => {
                        self.source.set_retrigger(mode);
                    }
                    AutoCmd::OscParam { osc_index, param, value } => {
                        self.source.set_osc_param(osc_index, param, value);
                    }
                    AutoCmd::OscWaveform { osc_index, waveform } => {
                        self.source.set_osc_waveform(osc_index, waveform);
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

        // Render source (mono) and widen to stereo
        let mono = self.source.render_sample();
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
