use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::engine::Engine;
use crate::live::scene::Scene;

/// Live-coding entry point with subsecond hot-reload.
///
/// Sets up audio output and re-evaluates the scene function in a loop. The
/// scene is stateful — per-track hot-function pointer comparison skips
/// unchanged tracks between edits; changed tracks are diffed and only the
/// differences are sent to the engine.
pub fn live(bpm: f32, scene_fn: fn(&mut Scene)) -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "autosynth=debug".parse().unwrap()),
        )
        .init();

    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .context("no audio output device available")?;
    let supported = device.default_output_config()?;
    let sample_rate = supported.sample_rate().0 as f32;
    let channels = supported.channels() as usize;

    let (engine, handle) = Engine::new(sample_rate, channels, bpm);
    let stream = engine
        .build_stream(&device, &supported.into())
        .context("failed to build audio stream")?;
    stream.play()?;

    // subsecond needs this connection for the ASLR reference used by hot-patching.
    dioxus_devtools::connect_subsecond();

    #[cfg(feature = "midi")]
    {
        if crate::live::midi::connect(handle.clone()).is_some() {
            println!("MIDI input connected");
        } else {
            println!("no MIDI input device found");
        }
    }

    println!("autosynth live @ {bpm} BPM — Ctrl+C to stop");

    let mut scene = Scene::new(bpm, sample_rate, handle);
    loop {
        subsecond::call(|| {
            scene_fn(&mut scene);
            scene.finish_frame();
        });
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}
