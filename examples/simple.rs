use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rustsynth::{duration, prelude::*};

fn chords() -> Clip {
    Clip::looped("chords", 8.0).build(|s| {
        // s.note(b(0.0), E3, 0.5, duration::s());
        s.chord(b(0.0), &diatonic_triad(E4, &MINOR, 1), 0.5, duration::s());
        s.chord(b(2.0), &diatonic_triad(E4, &MINOR, 6), 0.5, duration::s());
        s.chord(b(4.0), &diatonic_triad(E4, &MINOR, 2), 0.5, duration::s());
        s.chord(b(6.0), &diatonic_triad(E4, &MINOR, 5), 0.5, duration::s());
    })
}

fn bass() -> Clip {
    Clip::looped("bass", 8.0).build(|s| {
        // s.note(b(0.0), E2, 0.3, 0.25);
        // s.note(b(0.25), E2, 0.9, h());
    })
}

fn main() -> Result<()> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .context("No output device available")?;
    let supported = device.default_output_config()?;
    let sample_rate = supported.sample_rate().0 as f32;
    let channels = supported.channels() as usize;
    let tempo = Tempo::new(120.0);

    let (engine, handle) = Engine::new(sample_rate, channels, tempo);

    // Create tracks with different patches
    handle.add_track_with_polyphony("chords", Patch::new().oscillators(vec![Oscillator::new(Waveform::Square),
    ]).attack(0.05), 4);
    handle.add_track(
        "bass",
        Patch::new()
            .oscillators(vec![Oscillator::new(Waveform::Saw)])
            .cutoff(800.0),
    );

    // Add effects to tracks
    handle.add_effect("chords", Delay::tempo_synced(0.5, 120.0, 0.5, 0.3, sample_rate));
    handle.add_effect("bass", Distortion::new(2.0, 0.3));

    // Launch clips onto specific tracks
    handle.launch("chords", chords());
    handle.launch("bass", bass());

    // Engine moves into the audio callback, no Arc<Mutex<...>> needed
    let stream = engine
        .build_stream(&device, &supported.into())
        .context("Failed to build audio stream")?;
    stream.play()?;

    println!("Playing at {} BPM. Press Ctrl+C to stop.", tempo.bpm);

    loop {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}
