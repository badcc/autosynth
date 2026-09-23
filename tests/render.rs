//! End-to-end offline render: scene → engine → WAV, no audio device.

use autosynth::prelude::*;

const INTRO: Section = Section::start(1);
const BUILD: Section = INTRO.then(2);

fn energy() -> Signal {
    curve().hold(INTRO, 0.2).rise(BUILD, 0.2, 1.0).ease().signal()
}

fn kick(t: &mut Track) {
    t.synth(presets::kick());
    t.play(bars(1.0), |p| {
        p.hits(C1, "x... x... x... x...");
    });
}

fn hats(t: &mut Track) {
    t.synth(presets::hat());
    t.gain(energy().range(0.1, 0.4));
    t.to(room);
    t.play(bars(1.0), |p| {
        p.hits(C5, ".x.x .x.x .x.x .xxx").vel("x g");
        p.degrade(0.3);
    });
}

fn bass(t: &mut Track) {
    t.synth(presets::acid().ladder(300.0 + env(0.0, 0.2) * vel() * energy().range(500.0, 4000.0), 0.7));
    t.duck(kick).depth(0.5);
    t.fx(|c| {
        c.drive(2.0).saturate().mix(0.4);
        c.send(room, 0.2);
    });
    t.play(bars(1.0), |p| {
        let root = p.pick(&[1, 4, 5]);
        p.root(root).seq("1 1 8 1  b3 1 . 5  1 1 b7 1  5 . 8 1").slide("..x. .... ..x. ....").vel("X... ..X.");
    });
}

fn pads(t: &mut Track) {
    t.synth(presets::supersaw(5, 0.2).attack(0.3).release(1.0));
    t.gain(0.3);
    t.to(room);
    t.play(bars(2.0), |p| {
        p.chords("i VI", bars(1.0)).voice_lead();
    });
}

fn room(b: &mut Bus) {
    b.fx(chains::space);
}

fn song(s: &mut Scene) {
    s.tempo(128.0);
    s.key(A2, MINOR);
    s.track(kick);
    s.track(hats);
    s.track(bass);
    s.track(pads);
    s.bus(room);
    s.master(chains::glue);
}

fn render_bytes(scene: fn(&mut Scene), bars: f32, name: &str) -> Vec<u8> {
    let path = std::env::temp_dir().join(name);
    autosynth::render(scene, bars, &path).expect("render");
    let bytes = std::fs::read(&path).expect("read");
    let _ = std::fs::remove_file(&path);
    bytes
}

#[test]
fn full_scene_renders_audio() {
    let path = std::env::temp_dir().join("autosynth_test_song.wav");
    autosynth::render(song, 4.0, &path).expect("render");
    let reader = hound::WavReader::open(&path).expect("wav");
    assert_eq!(reader.spec().channels, 2);
    let samples: Vec<f32> = reader.into_samples::<f32>().map(|s| s.unwrap()).collect();
    // 4 bars at 128 BPM = 7.5 s.
    assert_eq!(samples.len(), (7.5 * 44_100.0) as usize * 2);
    let peak = samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    assert!(peak > 0.05 && peak <= 1.0, "peak {peak}");
    assert!(samples.iter().all(|s| s.is_finite()));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn renders_are_byte_identical() {
    // Randomness (pick, degrade, `?`) is seeded per track and cycle.
    let a = render_bytes(song, 4.0, "autosynth_test_det_a.wav");
    let b = render_bytes(song, 4.0, "autosynth_test_det_b.wav");
    assert!(a == b, "seeded renders must be byte-identical");
}
