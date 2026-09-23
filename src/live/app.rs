use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::engine::run::Engine;
use crate::live::scene::Scene;

/// Live-coding entry point with subsecond hot reload. Plays at 120 BPM until
/// the scene sets `s.tempo(..)`.
///
/// Run with `-- --render <bars> <file.wav>` to bounce the song offline instead.
pub fn live(scene_fn: fn(&mut Scene)) -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--render") {
        let bars = args.get(i + 1).and_then(|s| s.parse().ok()).unwrap_or(16.0);
        let path = args.get(i + 2).cloned().unwrap_or_else(|| "out.wav".into());
        crate::live::render::render(scene_fn, bars, &path)?;
        println!("rendered {bars} bars to {path}");
        return Ok(());
    }

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "autosynth=debug".parse().unwrap()))
        .init();

    let host = cpal::default_host();
    let device = host.default_output_device().context("no audio output device available")?;
    let supported = device.default_output_config()?;
    let sample_rate = supported.sample_rate().0 as f32;
    let channels = supported.channels() as usize;

    let (engine, handle) = Engine::new(sample_rate, channels, 120.0);
    let stream = engine.build_stream(&device, &supported.into()).context("failed to build audio stream")?;
    stream.play()?;

    // subsecond needs this connection for the ASLR reference used by hot-patching.
    dioxus_devtools::connect_subsecond();

    #[cfg(feature = "midi")]
    let _midi = {
        let conn = crate::live::midi::connect(handle.clone());
        println!("{}", if conn.is_some() { "MIDI input connected" } else { "no MIDI input device found" });
        conn
    };

    println!("autosynth live — Ctrl+C to stop");

    let mut scene = Scene::new(sample_rate, handle);
    loop {
        subsecond::call(|| {
            // Resolve the scene fn through the jump table every frame: `scene_fn`
            // holds the base binary's address, so a plain call would run the
            // stale body forever.
            subsecond::HotFn::current(scene_fn).call((&mut scene,));
            scene.finish_frame();
        });
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}
