use std::f32::consts::TAU;

use anyhow::Result;
use autosynth::prelude::*;

fn main() -> Result<()> {
    autosynth::live(120.0, scene)
}

fn scene(s: &mut Scene) {
    s.track(chords);
    s.track(lead);
    s.track(bass);
}

fn chords(t: &mut Track) {
    t.osc(Waveform::Triangle, 1.0);
    t.gain(0.5);
    t.attack(0.05);
    t.resonance(0.3);

    t.delay(|d| {
        d.beats(0.5).feedback(0.4).mix(0.4).ping_pong();
    });

    t.every(8.0, |p| {
        p.chord(b(0.0), &diatonic_triad(E3, &MAJOR, 1), 0.5, s());
        p.chord(b(2.0), &diatonic_triad(E3, &MAJOR, 6), 0.7, s());
        p.chord(b(4.0), &diatonic_triad(E3, &MAJOR, 2), 0.6, s());
        p.chord(b(6.0), &diatonic_triad(E3, &MAJOR, 5), 0.5, s());
    });
}

fn lead(t: &mut Track) {
    t.osc(Waveform::Sine, 0.5);
    t.osc(Waveform::Saw, 0.5).detune(0.05);
    t.gain(0.6);
    t.attack(0.01);
    t.sustain(1.0);
    t.decay(0.0);
    t.release(0.01);
    t.retrigger(RetriggerMode::Soft);
    t.filter_type(FilterType::Lowpass);
    t.cutoff(|c: Clock| 1000.0 + 300.0 * (c.beat * 0.025 * TAU).sin());
    t.resonance(0.1);
    t.polyphony(1);

    t.distortion(|d| {
        d.drive(12.0).fuzz().tone(0.8).bias(0.2).output(0.1);
    });

    t.every(8.0, |p| {
        p.note(b(0.0), degree(E3, &MAJOR, 1), 1.0, q() - 0.125);
        p.note(b(q()), degree(E4, &MAJOR, 6), 0.5, s());
        p.note(b(q() * 2.0), degree(E3, &MAJOR, 2), 1.0, q() - 0.125);
        p.note(b(q() * 3.0), degree(E4, &MAJOR, 5), 0.5, s());

        // Random notes that regenerate each loop
        for i in 0..3 {
            p.note(
                b(7.0 + i as f32 * 0.125 + rand::random_range(0.0..0.125)),
                degree(E4, &MAJOR, 3 - i),
                1.0 - i as f32 * 0.3,
                s(),
            );
        }
    });
}

fn bass(t: &mut Track) {
    t.osc(Waveform::Saw, 1.0);
    t.cutoff(800.0);
    t.gain(0.5);

    t.distortion(|d| {
        d.drive(2.0).mix(0.3);
    });

    t.every(8.0, |p| {
        p.note(b(0.0), E2, 0.9, h());
        p.note(b(6.0), E2 + 2, 0.9, 1.9);
    });
}
