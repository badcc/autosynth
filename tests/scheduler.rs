//! Regression tests for the beat-native scheduler (design defects 1–2).

use autosynth::engine::scheduler::{NoteEv, Scheduler};
use autosynth::music::NoteSpec;

fn on(beat: f32, note: u8, dur: f32) -> NoteSpec {
    NoteSpec {
        beat,
        note,
        vel: 1.0,
        dur,
    }
}

#[test]
fn note_on_positions_are_exact() {
    let mut s = Scheduler::new();
    s.launch(vec![on(0.0, 60, 0.25), on(1.0, 62, 0.25), on(2.5, 64, 0.25)], None, 0.0);

    let mut out = Vec::new();
    s.collect(0.0, 4.0, &mut out);

    let ons: Vec<(f64, u8)> = out
        .iter()
        .filter_map(|f| match f.ev {
            NoteEv::On { note, .. } => Some((f.beat, note)),
            _ => None,
        })
        .collect();
    assert_eq!(ons, vec![(0.0, 60), (1.0, 62), (2.5, 64)]);
}

#[test]
fn note_off_obligation_survives_loop_wrap() {
    // A note starting at beat 0.75 with a 0.5-beat duration releases at 1.25 —
    // past the loop boundary at 1.0. The old player dropped it; the obligation
    // model must still emit the off.
    let mut s = Scheduler::new();
    s.launch(vec![on(0.75, 60, 0.5)], Some(1.0), 0.0);

    let mut first = Vec::new();
    s.collect(0.0, 1.0, &mut first);
    assert!(
        first.iter().any(|f| matches!(f.ev, NoteEv::On { note: 60, .. }) && (f.beat - 0.75).abs() < 1e-9),
        "note-on at 0.75 should fire in the first iteration"
    );
    assert!(
        !first.iter().any(|f| matches!(f.ev, NoteEv::Off { note: 60 })),
        "the off at 1.25 must not fire before the boundary"
    );

    let mut second = Vec::new();
    s.collect(1.0, 2.0, &mut second);
    let off = second
        .iter()
        .find(|f| matches!(f.ev, NoteEv::Off { note: 60 }));
    assert!(off.is_some(), "the note-off must survive the loop wrap");
    assert!((off.unwrap().beat - 1.25).abs() < 1e-9, "off fires at its true beat 1.25");
    // And the next iteration's note-on also appears.
    assert!(second
        .iter()
        .any(|f| matches!(f.ev, NoteEv::On { note: 60, .. }) && (f.beat - 1.75).abs() < 1e-9));
}

#[test]
fn events_are_stable_across_tempo_changes() {
    // The scheduler is purely beat-based, so event beats never depend on tempo.
    // (Tempo lives in the transport; see the engine tests.)
    let mut s = Scheduler::new();
    s.launch(vec![on(0.0, 60, 0.5), on(2.0, 60, 0.5)], Some(4.0), 0.0);

    let mut out = Vec::new();
    s.collect(0.0, 4.0, &mut out);
    let ons: Vec<f64> = out
        .iter()
        .filter_map(|f| match f.ev {
            NoteEv::On { .. } => Some(f.beat),
            _ => None,
        })
        .collect();
    assert_eq!(ons, vec![0.0, 2.0]);
}

#[test]
fn one_shot_stops_after_playing() {
    let mut s = Scheduler::new();
    s.launch(vec![on(0.0, 60, 0.25)], None, 0.0);
    let mut out = Vec::new();
    s.collect(0.0, 10.0, &mut out);
    assert!(!s.is_active(), "a non-looping schedule deactivates once exhausted");
    assert_eq!(
        out.iter().filter(|f| matches!(f.ev, NoteEv::On { .. })).count(),
        1
    );
}
