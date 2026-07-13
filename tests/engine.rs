//! End-to-end engine tests: note onset timing through the real render path,
//! including a mid-flight tempo change (design defect 1).

use autosynth::engine::command::{BuiltSource, Command, Playback, TrackBuild};
use autosynth::engine::Engine;
use autosynth::model::PatchSpec;
use autosynth::model::track::Swing;
use autosynth::music::{NoteSpec, Phrase};

fn synth_build(notes: Vec<NoteSpec>) -> TrackBuild {
    TrackBuild {
        source: BuiltSource::Synth,
        patch: PatchSpec::default(),
        polyphony: 4,
        gain: 1.0,
        pan: 0.0,
        mute: false,
        fx: Vec::new(),
        fx_enabled: Vec::new(),
        automations: Vec::new(),
        playback: Playback::OneShot { notes, swing: None },
        seed: 0,
    }
}

fn first_audible(buf: &[f32]) -> Option<usize> {
    buf.iter().position(|s| s.abs() > 1e-3)
}

#[test]
fn note_onset_lands_at_the_right_sample() {
    let sr = 44_100.0;
    let (mut engine, handle) = Engine::new(sr, 1, 120.0);
    handle.send(Command::AddTrack {
        name: "t".into(),
        build: Box::new(synth_build(vec![NoteSpec {
            beat: 1.0,
            note: 69,
            vel: 1.0,
            dur: 0.5,
        }])),
    });

    let mut buf = vec![0.0f32; 44_100]; // 1 second, mono
    engine.render(&mut buf);

    // Beat 1.0 at 120 BPM = 0.5 s = 22050 samples. Allow attack + one control block.
    let onset = first_audible(&buf).expect("note should sound");
    assert!(
        (21_900..=22_200).contains(&onset),
        "onset at sample {onset}, expected ~22050"
    );
}

#[test]
fn tempo_change_moves_future_notes_without_teleporting() {
    let sr = 44_100.0;
    let (mut engine, handle) = Engine::new(sr, 1, 120.0);
    handle.send(Command::AddTrack {
        name: "t".into(),
        build: Box::new(synth_build(vec![NoteSpec {
            beat: 1.0,
            note: 69,
            vel: 1.0,
            dur: 0.5,
        }])),
    });

    let mut buf = vec![0.0f32; 44_100];
    // First 0.25 s at 120 BPM covers beats 0..0.5.
    engine.render(&mut buf[..11_025]);
    // Double the tempo: the remaining 0.5 beats now take 0.125 s.
    handle.send(Command::SetTempo(240.0));
    engine.render(&mut buf[11_025..]);

    // Expected onset: 0.25 s + 0.125 s = 0.375 s = 16537 samples.
    let onset = first_audible(&buf).expect("note should sound");
    assert!(
        (16_400..=16_700).contains(&onset),
        "onset at sample {onset}, expected ~16537 after tempo doubling"
    );
}

#[test]
fn swing_delays_notes_on_odd_grid_positions() {
    // A note on beat 0.25 (an odd 16th) with grid=0.25, amount=0.75 shifts late
    // by (0.75 - 0.5) * 2 * 0.25 = 0.125 beats → it lands on beat 0.375.
    // At 120 BPM that is 0.375 * 0.5 s = 0.1875 s = 8268 samples.
    let sr = 44_100.0;
    let (mut engine, handle) = Engine::new(sr, 1, 120.0);
    let build = TrackBuild {
        source: BuiltSource::Synth,
        patch: PatchSpec::default(),
        polyphony: 4,
        gain: 1.0,
        pan: 0.0,
        mute: false,
        fx: Vec::new(),
        fx_enabled: Vec::new(),
        automations: Vec::new(),
        playback: Playback::Pattern {
            func: Box::new(|p: &mut Phrase| {
                p.at(0.25);
                p.note(69, 0.25);
            }),
            loop_len: 4.0,
            swing: Some(Swing { grid: 0.25, amount: 0.75 }),
        },
        seed: 0,
    };
    handle.send(Command::AddTrack {
        name: "t".into(),
        build: Box::new(build),
    });

    let mut buf = vec![0.0f32; 44_100];
    engine.render(&mut buf);

    let onset = first_audible(&buf).expect("swung note should sound");
    assert!(
        (8_150..=8_450).contains(&onset),
        "onset at sample {onset}, expected ~8268 after swing"
    );
}

#[test]
fn silence_when_nothing_scheduled() {
    let sr = 44_100.0;
    let (mut engine, _handle) = Engine::new(sr, 2, 120.0);
    let mut buf = vec![0.0f32; 4_096];
    engine.render(&mut buf);
    assert!(buf.iter().all(|s| *s == 0.0), "no tracks → digital silence");
}
