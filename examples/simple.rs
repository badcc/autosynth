use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rustsynth::{duration, prelude::*};

fn chords() -> Clip {
    Clip::looped("chords", 8.0).build(|s| {
        // s.note(b(0.0), E3, 0.5, duration::s());
        s.chord(b(0.0), &diatonic_triad(E3, &MINOR, 1), 0.5, duration::s());
        s.chord(b(2.0), &diatonic_triad(E3, &MINOR, 6), 0.5, duration::s());
        s.chord(b(4.0), &diatonic_triad(E3, &MINOR, 2), 0.5, duration::s());
        s.chord(b(6.0), &diatonic_triad(E3, &MINOR, 5), 0.5, duration::s());
    })
}

fn lead() -> Clip {
    Clip::looped("lead", 4.0).build(|s| {
        // s.note(b(0.0), E3, 0.5, duration::s());
        s.note(
            b(0.0),
            degree(E3, &MINOR, 1),
            1.0,
            duration::q() - duration::t(),
        );
        // s.note(b(duration::q()), degree(E3, &MINOR, 6), 1.0, duration::s());
        // s.note(
        //     b(duration::q() * 2.0),
        //     degree(E3, &MINOR, 2),
        //     1.0,
        //     duration::q() - duration::t(),
        // );
        // s.note(
        //     b(duration::q() * 3.0),
        //     degree(E3, &MINOR, 5),
        //     1.0,
        //     duration::s(),
        // );
        // s.note(b(duration::s()*2.0), E4+2, 1.0, duration::s());
        // s.note(b(duration::s()*3.0), E4+3, 1.0, duration::q());
        // s.chord(b(2.0), &diatonic_triad(E4, &MINOR, 6), 0.5, duration::s());
        // s.chord(b(4.0), &diatonic_triad(E4, &MINOR, 2), 0.5, duration::s());
        // s.chord(b(6.0), &diatonic_triad(E4, &MINOR, 5), 0.5, duration::s());
    })
}

fn bass() -> Clip {
    Clip::looped("bass", 8.0).build(|s| {
        s.note(b(0.0), E2, 0.9, h());
        s.note(b(6.0), E2 + 2, 0.9, 2.0);
        // TODO: FIX: bug .. if note is playing when we loop, it gets stuck forever..
        // e.g. 6.0 + 2.0 = 8.0, our loop beats
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
    handle.add_track_with_polyphony(
        "chords",
        Patch::new()
            .oscillators(vec![
                Oscillator::new(Waveform::Triangle).level(1.0),
                // Oscillator::new(Waveform::Triangle)
                //     .phase_offset(0.2)
                //     .level(0.5),
            ])
            // .master_gain(1.0)
            .attack(0.05)
            // .filter_type(FilterType::Lowpass)
            // .cutoff(1200.0)
            .resonance(0.3),
        4,
    );
    handle.add_track_with_polyphony(
        "lead",
        Patch::new()
            .oscillators(vec![
                // Oscillator::new(Waveform::Sine),
                Oscillator::new(Waveform::Sine).level(0.5),
                Oscillator::new(Waveform::Saw)
                    .level(0.5)
                    .detune(0.05)
                    .phase_offset(0.1),
            ])
            .attack(0.01)
            .sustain(1.0)
            .decay(0.0)
            .release(0.01)
            .retrigger(RetriggerMode::Soft)
            .filter_type(FilterType::Lowpass)
            .cutoff(600.0)
            .resonance(0.1),
        1,
    );
    handle.add_track(
        "bass",
        Patch::new()
            .oscillators(vec![Oscillator::new(Waveform::Saw)])
            .cutoff(800.0),
    );

    // Add effects to tracks

    handle.add_effect("bass", Distortion::new(2.0, 0.3));
    handle.add_effect(
        "lead",
        Distortion::new(12.0, 1.0)
            .mode(DistortionMode::Fuzz)
            .tone(0.8)
            .bias(0.2)
            .output_gain(0.1),
    );
    handle.add_effect(
        "chords",
        Delay::tempo_synced(0.5, 120.0, 0.4, 0.4, sample_rate).mode(DelayMode::PingPong),
    );

    // Launch clips onto specific tracks
    handle.launch("chords", chords());
    handle.launch("bass", bass());
    // handle.launch("lead", lead());

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
