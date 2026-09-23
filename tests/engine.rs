//! End-to-end engine behavior through the public scene API: timing, tempo,
//! swing, per-voice modulation, glide, sidechain, sends, song position, and
//! chain reconciliation.

use autosynth::dsp::StereoFrame;
use autosynth::dsp::effects::FxCtx;
use autosynth::engine::chain::{FxChain, NodeUpdate, SendTarget};
use autosynth::engine::command::{Command, EngineHandle};
use autosynth::engine::run::Engine;
use autosynth::live::chain_builder::Chain;
use autosynth::prelude::*;

const SR: f32 = 44_100.0;

/// Evaluate a scene once and return the engine plus a command handle.
fn start(scene: fn(&mut Scene)) -> (Engine, EngineHandle) {
    let (engine, handle) = Engine::new(SR, 1, 120.0);
    let mut s = Scene::new(SR, handle.clone());
    scene(&mut s);
    s.finish_frame();
    (engine, handle)
}

fn render(scene: fn(&mut Scene), seconds: f32) -> Vec<f32> {
    let (mut engine, _h) = start(scene);
    let mut buf = vec![0.0; (seconds * SR) as usize];
    engine.render(&mut buf);
    buf
}

fn first_audible(buf: &[f32]) -> Option<usize> {
    buf.iter().position(|s| s.abs() > 1e-3)
}

fn rms(buf: &[f32]) -> f32 {
    (buf.iter().map(|s| s * s).sum::<f32>() / buf.len().max(1) as f32).sqrt()
}

/// High-frequency content: RMS of the first difference relative to the signal.
fn brightness(buf: &[f32]) -> f32 {
    let diff: Vec<f32> = buf.windows(2).map(|w| w[1] - w[0]).collect();
    rms(&diff) / rms(buf).max(1e-9)
}

/// Zero crossings per second — a pitch proxy for a sine.
fn crossings_hz(buf: &[f32]) -> f32 {
    let n = buf.windows(2).filter(|w| (w[0] < 0.0) != (w[1] < 0.0)).count();
    n as f32 / 2.0 / (buf.len() as f32 / SR)
}

fn at(seconds: f32) -> usize {
    (seconds * SR) as usize
}

// ── Timing ──

fn onset_track(t: &mut Track) {
    t.synth(Synth::new().osc(Sine).adsr(0.0, 0.0, 1.0, 0.01));
    t.play(4.0, |p| {
        p.at(1.0).note(A4, 0.5);
    });
}

fn onset_scene(s: &mut Scene) {
    s.track(onset_track);
}

#[test]
fn note_onset_lands_at_the_right_sample() {
    // Beat 1.0 at 120 BPM = 0.5 s = 22050 samples.
    let onset = first_audible(&render(onset_scene, 1.0)).expect("note should sound");
    assert!((22_000..=22_150).contains(&onset), "onset at {onset}");
}

#[test]
fn tempo_change_moves_future_notes() {
    let (mut engine, handle) = start(onset_scene);
    let mut buf = vec![0.0f32; 44_100];
    engine.render(&mut buf[..11_025]); // beats 0..0.5
    handle.send(Command::SetTempo(240.0));
    engine.render(&mut buf[11_025..]);
    // Remaining half beat takes 0.125 s: onset at 0.375 s.
    let onset = first_audible(&buf).expect("note should sound");
    assert!((16_450..=16_650).contains(&onset), "onset at {onset}");
}

fn swung(t: &mut Track) {
    t.synth(Synth::new().osc(Sine).adsr(0.0, 0.0, 1.0, 0.01));
    t.swing(N16, 0.75);
    t.play(4.0, |p| {
        p.at(0.25).note(A4, 0.25);
    });
}

#[test]
fn swing_delays_odd_grid_positions() {
    fn scene(s: &mut Scene) {
        s.track(swung);
    }
    // 0.25 + (0.75 - 0.5) * 2 * 0.25 = 0.375 beats = 8268 samples.
    let onset = first_audible(&render(scene, 1.0)).expect("swung note");
    assert!((8_200..=8_350).contains(&onset), "onset at {onset}");
}

#[test]
fn silence_when_nothing_scheduled() {
    let (mut engine, _h) = Engine::new(SR, 2, 120.0);
    let mut buf = vec![0.0f32; 4_096];
    engine.render(&mut buf);
    assert!(buf.iter().all(|s| *s == 0.0));
}

// ── Voice modulation ──

fn env_track(t: &mut Track) {
    t.synth(Synth::new().osc(Saw).lowpass(150.0 + env(0.0, 0.1) * 8000.0, 0.0).adsr(0.0, 0.0, 1.0, 0.01));
    t.play(4.0, |p| {
        p.note(A2, 2.0);
    });
}

#[test]
fn filter_envelope_brightens_the_onset() {
    fn scene(s: &mut Scene) {
        s.track(env_track);
    }
    let buf = render(scene, 0.6);
    let early = brightness(&buf[at(0.005)..at(0.03)]);
    let late = brightness(&buf[at(0.4)..at(0.55)]);
    assert!(early > late * 2.0, "early {early} vs late {late}");
}

fn vel_track(t: &mut Track, v: f32) {
    t.synth(Synth::new().osc(Saw).lowpass(150.0 + vel() * 8000.0, 0.0).adsr(0.0, 0.0, 1.0, 0.01).gain(1.0));
    t.play(4.0, move |p| {
        p.note(A2, 2.0).vel(v);
    });
}

fn loud(t: &mut Track) {
    vel_track(t, 1.0)
}

fn soft(t: &mut Track) {
    vel_track(t, 0.1)
}

#[test]
fn velocity_opens_the_filter() {
    fn a(s: &mut Scene) {
        s.track(loud);
    }
    fn b(s: &mut Scene) {
        s.track(soft);
    }
    let (x, y) = (render(a, 0.3), render(b, 0.3));
    assert!(brightness(&x[at(0.1)..]) > brightness(&y[at(0.1)..]) * 1.5);
}

fn slider(t: &mut Track) {
    t.synth(Synth::new().osc(Sine).adsr(0.0, 0.0, 1.0, 0.01).mono().glide(0.15));
    t.play(8.0, |p| {
        p.grid(N4).seq("1 8 _ _").slide("x.");
    });
}

#[test]
fn slides_glide_instead_of_jumping() {
    fn scene(s: &mut Scene) {
        s.track(slider);
    }
    // Key C4 by default: C4 (261 Hz) slides to C5 (523 Hz) at beat 1.0 (0.5 s).
    let buf = render(scene, 1.4);
    let first = crossings_hz(&buf[at(0.2)..at(0.45)]);
    let during = crossings_hz(&buf[at(0.52)..at(0.62)]);
    let settled = crossings_hz(&buf[at(1.1)..at(1.35)]);
    assert!((first - 261.6).abs() < 10.0, "first {first}");
    assert!(during > first + 20.0 && during < settled - 20.0, "mid-glide {during}");
    assert!((settled - 523.3).abs() < 12.0, "settled {settled}");
}

fn locked(t: &mut Track) {
    t.synth(Synth::new().osc(Saw).lowpass(8000.0, 0.0).adsr(0.0, 0.0, 1.0, 0.01));
    t.play(4.0, |p| {
        p.grid(1.0).seq("1 1").dur(0.9).cutoff("200 .");
    });
}

#[test]
fn cutoff_locks_apply_per_note() {
    fn scene(s: &mut Scene) {
        s.track(locked);
    }
    let buf = render(scene, 1.0);
    let dark = brightness(&buf[at(0.1)..at(0.4)]);
    let bright = brightness(&buf[at(0.6)..at(0.9)]);
    assert!(bright > dark * 2.0, "locked note {dark} vs unlocked {bright}");
}

// ── Sidechain, routing, sends ──

fn trigger(t: &mut Track) {
    t.synth(Synth::new().osc(Sine));
    t.gain(0.0); // silent; its notes still trigger the duck
    t.play(4.0, |p| {
        p.at(1.0).note(C3, 0.1);
    });
}

fn ducked(t: &mut Track) {
    t.synth(Synth::new().osc(Sine).adsr(0.0, 0.0, 1.0, 0.01));
    t.duck(trigger).depth(0.9).release(N8);
    t.play(4.0, |p| {
        p.note(A3, 4.0);
    });
}

#[test]
fn duck_dips_on_the_source_note() {
    fn scene(s: &mut Scene) {
        s.track(trigger);
        s.track(ducked);
    }
    let buf = render(scene, 1.0);
    let before = rms(&buf[at(0.3)..at(0.48)]);
    let after = rms(&buf[at(0.51)..at(0.53)]);
    let recovered = rms(&buf[at(0.85)..at(0.98)]);
    assert!(after < before * 0.3, "dip: before {before}, after {after}");
    assert!(recovered > before * 0.9, "recovers: {recovered}");
}

fn wet(b: &mut Bus) {
    b.gain(1.0);
}

fn sender(t: &mut Track) {
    t.synth(Synth::new().osc(Sine).adsr(0.0, 0.0, 1.0, 0.01));
    t.fx(|c| {
        c.send(wet, 1.0);
        c.drive(0.0); // silence the track itself after the send
    });
    t.play(4.0, |p| {
        p.note(A3, 4.0);
    });
}

#[test]
fn sends_reach_the_bus() {
    fn with_bus(s: &mut Scene) {
        s.track(sender);
        s.bus(wet);
    }
    fn without_bus(s: &mut Scene) {
        s.track(sender);
    }
    assert!(rms(&render(with_bus, 0.3)) > 0.1, "the bus carries the send");
    assert!(rms(&render(without_bus, 0.3)) < 1e-4, "no bus, nothing after the silencer");
}

fn muted_bus(b: &mut Bus) {
    b.mute(true);
}

fn routed(t: &mut Track) {
    t.synth(Synth::new().osc(Sine));
    t.to(muted_bus);
    t.play(4.0, |p| {
        p.note(A3, 4.0);
    });
}

#[test]
fn routing_goes_through_the_bus() {
    fn scene(s: &mut Scene) {
        s.track(routed);
        s.bus(muted_bus);
    }
    assert!(rms(&render(scene, 0.3)) < 1e-4);
}

// ── Song position ──

const DROP: Section = Section::at(8, 8);

fn drop_only(t: &mut Track) {
    t.synth(Synth::new().osc(Sine));
    t.gain(during(DROP));
    t.play(4.0, |p| {
        p.note(A3, 4.0);
    });
}

#[test]
fn jump_moves_the_song_position() {
    fn at_start(s: &mut Scene) {
        s.track(drop_only);
    }
    fn jumped(s: &mut Scene) {
        s.track(drop_only);
        s.jump(DROP);
    }
    assert!(rms(&render(at_start, 0.3)) < 1e-4, "bar 0 is before the drop");
    assert!(rms(&render(jumped, 0.3)) > 0.1, "after the jump the drop plays");
}

// ── Chain reconciliation ──

struct NoSends;

impl SendTarget for NoSends {
    fn send(&mut self, _bus: &str, _buf: &[StereoFrame], _gain: f32) {}
}

fn fx_ctx() -> FxCtx {
    FxCtx { sample_rate: SR, bpm: 120.0, beat: 0.0, beats_per_sample: 2.0 / SR as f64 }
}

fn tail_after_edit(edit: fn(&mut Chain)) -> f32 {
    let a = Chain::from_fn(|c| {
        c.delay(0.25).feedback(0.9).mix(1.0);
    });
    let b = Chain::from_fn(edit);
    let mut chain = FxChain::build(&a, SR);
    let knobs = [0.0; 128];
    let mut block = vec![[0.0f32; 2]; 64];
    block[0] = [1.0, 1.0];
    chain.process(&mut block, &fx_ctx(), &knobs, &mut NoSends);
    chain.apply(FxChain::diff(&a, &b, SR));
    let mut energy = 0.0;
    for _ in 0..200 {
        let mut block = vec![[0.0f32; 2]; 64];
        chain.process(&mut block, &fx_ctx(), &knobs, &mut NoSends);
        energy += block.iter().map(|f| f[0].abs()).sum::<f32>();
    }
    energy
}

#[test]
fn parameter_edits_keep_the_delay_tail() {
    let kept = tail_after_edit(|c| {
        c.delay(0.25).feedback(0.5).mix(1.0);
    });
    let rebuilt = tail_after_edit(|c| {
        c.delay(0.25).feedback(0.5).mix(1.0).ping_pong();
    });
    assert!(kept > 0.1, "same kind: the echo keeps ringing ({kept})");
    assert!(rebuilt < 1e-6, "a kind change rebuilds the node ({rebuilt})");
}

#[test]
fn diff_keeps_same_kind_and_rebuilds_others() {
    let a = Chain::from_fn(|c| {
        c.reverb().mix(0.2);
        c.drive(2.0);
    });
    let b = Chain::from_fn(|c| {
        c.reverb().mix(0.5);
        c.delay(0.5);
    });
    let d = FxChain::diff(&a, &b, SR);
    assert!(matches!(d[0], NodeUpdate::Keep { .. }));
    assert!(matches!(d[1], NodeUpdate::Build(_)));
}
