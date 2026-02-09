use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::engine::Engine;
use crate::scene::Scene;
use crate::score::Tempo;

/// Live-coding entry point with subsecond hot-reload.
///
/// Sets up audio output and evaluates the scene function in a loop.
/// Scene is stateful — per-track HotFn pointer comparison skips
/// unchanged tracks between patches.
pub fn live(bpm: f32, scene_fn: fn(&mut Scene)) -> Result<()> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .context("No audio output device available")?;
    let supported = device.default_output_config()?;
    let sample_rate = supported.sample_rate().0 as f32;
    let channels = supported.channels() as usize;

    let (engine, handle) = Engine::new(sample_rate, channels, Tempo::new(bpm));
    let stream = engine
        .build_stream(&device, &supported.into())
        .context("Failed to build audio stream")?;
    stream.play()?;

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
