//! End-to-end offline render: a scene function → engine → WAV file, no audio
//! device. Also exercises the Scene diff/build path.

use autosynth::prelude::*;

fn bass(t: &mut Track) {
    t.osc(Waveform::Saw, 0.8);
    t.cutoff(600.0);
    t.gain(0.7);
    t.every(2.0, |p| {
        p.note(0.0, E2, 0.9, 1.0);
        p.note(1.0, E2 + 7, 0.8, 1.0);
    });
}

fn scene(s: &mut Scene) {
    s.track(bass);
}

// Exercises the new surface from the design: Clock helpers, pan, a group bus
// with shared reverb.
fn kick(t: &mut Track) {
    t.osc(Waveform::Sine, 1.0);
    t.pan(-0.3);
    t.attack(0.001);
    t.decay(0.15);
    t.sustain(0.0);
    t.cutoff(|c: Clock| 400.0 + 300.0 * c.sin(4.0));
    t.every(1.0, |p| {
        p.note(0.0, E1, 1.0, 0.2);
    });
}

fn hat(t: &mut Track) {
    t.osc(Waveform::Noise, 0.5);
    t.pan(0.3);
    t.attack(0.001);
    t.decay(0.05);
    t.sustain(0.0);
    t.every(1.0, |p| {
        p.note(0.5, A5, 0.6, 0.1);
    });
}

fn grouped_scene(s: &mut Scene) {
    s.track(kick);
    s.track(hat);
    s.group("drums", |g| {
        g.track(kick);
        g.track(hat);
        g.gain(0.9);
        g.reverb(|r| {
            r.size(0.4).mix(0.25);
        });
    });
}

#[test]
fn group_bus_with_reverb_renders() {
    let dir = std::env::temp_dir();
    let path = dir.join("autosynth_test_group.wav");
    autosynth::render(128.0, grouped_scene, 4.0, &path).expect("group render should succeed");
    let reader = hound::WavReader::open(&path).expect("wav exists");
    let peak = reader
        .into_samples::<f32>()
        .map(|s| s.unwrap().abs())
        .fold(0.0f32, f32::max);
    assert!(peak > 0.01, "grouped scene should produce audio (peak {peak})");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn offline_render_writes_nonsilent_audio() {
    let dir = std::env::temp_dir();
    let path = dir.join("autosynth_test_render.wav");
    autosynth::render(120.0, scene, 2.0, &path).expect("render should succeed");

    let reader = hound::WavReader::open(&path).expect("wav should exist");
    let samples: Vec<f32> = reader.into_samples::<f32>().map(|s| s.unwrap()).collect();
    assert!(!samples.is_empty());
    let peak = samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    assert!(peak > 0.01, "rendered audio should not be silent (peak {peak})");

    let _ = std::fs::remove_file(&path);
}
