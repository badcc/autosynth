//! Instrument presets — plain functions returning a [`Synth`]. Tweak one by
//! chaining: `presets::supersaw(7, 0.2).attack(1.5).cutoff(energy().rangex(600.0, 6000.0))`.
//! Read them, copy them, make your own.

use crate::dsp::envelope::RetriggerMode;
use crate::dsp::waveform::Waveform::{Noise, Saw, Sine, Square, Triangle};
use crate::model::instrument::Synth;
use crate::music::signal::{env, vel};

/// A detuned saw stack — trance leads and big pads.
pub fn supersaw(voices: u8, spread: f32) -> Synth {
    Synth::new().osc(Saw.unison(voices, spread)).lowpass(9000.0, 0.1).adsr(0.02, 0.3, 0.8, 0.6)
}

/// A 303-style mono bass: saw into a resonant ladder with an accent-scaled
/// filter envelope. Slides glide.
pub fn acid() -> Synth {
    Synth::new()
        .osc(Saw)
        .ladder(300.0 + env(0.0, 0.2) * vel() * 3500.0, 0.8)
        .adsr(0.002, 0.3, 0.6, 0.03)
        .gain(0.55 + vel() * 0.45)
        .mono()
}

/// A bright pluck: the filter snaps shut after each note.
pub fn pluck() -> Synth {
    Synth::new()
        .osc(Saw.level(0.6))
        .osc(Square.level(0.4).semis(12.0))
        .lowpass(600.0 + env(0.0, 0.25) * vel() * 4000.0, 0.2)
        .adsr(0.002, 0.35, 0.0, 0.2)
}

/// A slow, soft pad.
pub fn pad() -> Synth {
    Synth::new()
        .osc(Saw.unison(5, 0.15).level(0.7))
        .osc(Triangle.level(0.5).semis(-12.0))
        .lowpass(2500.0, 0.1)
        .adsr(1.2, 1.0, 0.8, 2.5)
}

/// A clean sub bass.
pub fn sub() -> Synth {
    Synth::new().osc(Sine).osc(Triangle.level(0.3)).adsr(0.005, 0.1, 0.9, 0.08).mono()
}

/// Two detuned saw stacks beating against each other — the reese.
pub fn reese() -> Synth {
    Synth::new()
        .osc(Saw.unison(2, 0.3))
        .osc(Saw.unison(2, 0.5).semis(-12.0).level(0.6))
        .lowpass(900.0, 0.3)
        .adsr(0.01, 0.2, 0.9, 0.15)
        .mono()
}

/// A synthesized kick: a sine with a fast downward pitch sweep. Play it low
/// (`C1`).
pub fn kick() -> Synth {
    Synth::new()
        .osc(Sine)
        .pitch(env(0.0, 0.06) * 36.0)
        .adsr(0.001, 0.4, 0.0, 0.05)
        .retrigger(RetriggerMode::Hard)
}

/// A snare: tuned body plus noise.
pub fn snare() -> Synth {
    Synth::new()
        .osc(Triangle.level(0.5))
        .osc(Noise.level(0.7))
        .pitch(env(0.0, 0.03) * 12.0)
        .highpass(180.0, 0.0)
        .adsr(0.001, 0.18, 0.0, 0.05)
        .retrigger(RetriggerMode::Hard)
}

/// A closed hi-hat: high-passed noise with a short decay.
pub fn hat() -> Synth {
    Synth::new().osc(Noise).highpass(7000.0, 0.2).adsr(0.001, 0.05, 0.0, 0.03).retrigger(RetriggerMode::Hard)
}
