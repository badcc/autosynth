use crate::clip::Clip;
use crate::engine::EngineHandle;
use crate::event::Param;
use crate::patch::Patch;
use crate::scene::TrackDesc;
use crate::score::Score;

pub(crate) fn add_track(
    handle: &EngineHandle,
    name: &str,
    desc: &TrackDesc,
    bpm: f32,
    sample_rate: f32,
) {
    handle.add_track_with_polyphony(name, desc.patch.clone(), desc.polyphony);

    for effect_config in &desc.effects {
        handle.add_effect_boxed(name, effect_config.build(bpm, sample_rate));
    }

    if !desc.events.is_empty() {
        handle.launch(name, build_clip(desc));
    }
}

pub(crate) fn diff_track(
    handle: &EngineHandle,
    name: &str,
    old: &TrackDesc,
    new: &TrackDesc,
    bpm: f32,
    sample_rate: f32,
) {
    // ── Patch diff ──
    if old.patch != new.patch {
        diff_patch(handle, name, &old.patch, &new.patch);
    }

    // ── Polyphony change: requires full track rebuild ──
    if old.polyphony != new.polyphony {
        handle.remove_track(name);
        add_track(handle, name, new, bpm, sample_rate);
        return;
    }

    // ── Effects diff ──
    if old.effects != new.effects {
        handle.clear_effects(name);
        for effect_config in &new.effects {
            handle.add_effect_boxed(name, effect_config.build(bpm, sample_rate));
        }
    }

    // ── Clip diff ──
    if old.events != new.events || old.loop_beats != new.loop_beats {
        handle.stop(name, "live");
        if !new.events.is_empty() {
            handle.launch(name, build_clip(new));
        }
    }
}

fn diff_patch(handle: &EngineHandle, name: &str, old: &Patch, new: &Patch) {
    // Structural changes (oscillators, filter type, retrigger) require full patch swap
    if old.oscillators != new.oscillators
        || old.filter_type != new.filter_type
        || old.retrigger != new.retrigger
    {
        handle.set_patch(name, new.clone());
        return;
    }

    // Individual parameter changes — send SetParam for each
    if old.master_gain != new.master_gain {
        handle.set_param(name, Param::MasterGain, new.master_gain);
    }
    if old.attack != new.attack {
        handle.set_param(name, Param::Attack, new.attack);
    }
    if old.decay != new.decay {
        handle.set_param(name, Param::Decay, new.decay);
    }
    if old.sustain != new.sustain {
        handle.set_param(name, Param::Sustain, new.sustain);
    }
    if old.release != new.release {
        handle.set_param(name, Param::Release, new.release);
    }
    if old.cutoff != new.cutoff {
        handle.set_param(name, Param::Cutoff, new.cutoff);
    }
    if old.resonance != new.resonance {
        handle.set_param(name, Param::Resonance, new.resonance);
    }
    if old.lfo_rate != new.lfo_rate {
        handle.set_param(name, Param::LfoRate, new.lfo_rate);
    }
    if old.lfo_depth != new.lfo_depth {
        handle.set_param(name, Param::LfoDepth, new.lfo_depth);
    }
}

fn build_clip(desc: &TrackDesc) -> Clip {
    let mut clip = if let Some(beats) = desc.loop_beats {
        Clip::looped("live", beats)
    } else {
        Clip::new("live")
    };
    clip.score = Score::from_events(desc.events.clone());
    clip
}
