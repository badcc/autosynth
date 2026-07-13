//! End-to-end offline render: a scene function → engine → WAV file, no audio
//! device. Also exercises the Scene diff/build path.

use autosynth::prelude::*;

fn bass(t: &mut Track) {
    t.osc(Waveform::Saw, 0.8);
    t.cutoff(600.0);
    t.gain(0.7);
    t.every(2.0, |p| {
        p.note(E2, 1.0).vel(0.9);
        p.note(E2 + 7, 1.0).vel(0.8);
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
        p.note(E1, 0.2).vel(1.0);
    });
}

fn hat(t: &mut Track) {
    t.osc(Waveform::Noise, 0.5);
    t.pan(0.3);
    t.attack(0.001);
    t.decay(0.05);
    t.sustain(0.0);
    t.every(1.0, |p| {
        p.note(A5, 0.1).at(0.5).vel(0.6);
    });
}

fn grouped_scene(s: &mut Scene) {
    s.track(kick);
    s.track(hat);
    s.group("drums", |g| {
        g.track(kick);
        g.track(hat);
        g.gain(0.9);
        g.reverb().size(0.4).mix(0.25);
    });
}

// Seeded pattern randomness must produce byte-identical renders (DESIGN §10).
fn random_bass(t: &mut Track) {
    t.osc(Waveform::Saw, 0.8);
    t.cutoff(700.0);
    t.gain(0.6);
    t.every(2.0, |p| {
        for _ in 0..4 {
            let note = p.pick(&[E2, E2 + 3, E2 + 7, E2 + 10]);
            let vel = p.rand(0.6..0.9);
            p.note(note, 0.4).vel(vel);
        }
    });
}

fn random_scene(s: &mut Scene) {
    s.track(random_bass);
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
fn seeded_randomness_renders_are_byte_identical() {
    let dir = std::env::temp_dir();
    let a = dir.join("autosynth_test_rand_a.wav");
    let b = dir.join("autosynth_test_rand_b.wav");
    autosynth::render(120.0, random_scene, 4.0, &a).expect("render a");
    autosynth::render(120.0, random_scene, 4.0, &b).expect("render b");

    let ba = std::fs::read(&a).expect("read a");
    let bb = std::fs::read(&b).expect("read b");
    assert_eq!(ba, bb, "seeded renders must be byte-identical");

    let _ = std::fs::remove_file(&a);
    let _ = std::fs::remove_file(&b);
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
