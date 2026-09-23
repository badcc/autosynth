use std::path::Path;

use anyhow::{Context, Result};

use crate::engine::run::Engine;
use crate::live::scene::Scene;

/// Offline render: run the same engine with no audio device and write `bars`
/// bars of stereo audio to a WAV file. Deterministic and sample-exact — also
/// the testing story.
pub fn render(scene_fn: fn(&mut Scene), bars: f32, path: impl AsRef<Path>) -> Result<()> {
    let sample_rate = 44_100.0_f32;
    let (mut engine, handle) = Engine::new(sample_rate, 2, 120.0);
    let mut scene = Scene::new(sample_rate, handle);
    scene_fn(&mut scene);
    scene.finish_frame();
    drop(scene);

    // Drain the scene's commands (tempo included) before sizing the render.
    engine.render(&mut []);
    let bpm = engine.transport().bpm as f32;
    let frames = (bars * 4.0 * 60.0 / bpm * sample_rate) as usize;

    let spec = hound::WavSpec { channels: 2, sample_rate: sample_rate as u32, bits_per_sample: 32, sample_format: hound::SampleFormat::Float };
    let mut writer = hound::WavWriter::create(path.as_ref(), spec).context("failed to create WAV")?;
    const BLOCK: usize = 1024;
    let mut buf = vec![0.0f32; BLOCK * 2];
    let mut remaining = frames;
    while remaining > 0 {
        let n = remaining.min(BLOCK);
        let slice = &mut buf[..n * 2];
        engine.render(slice);
        for &s in slice.iter() {
            writer.write_sample(s)?;
        }
        remaining -= n;
    }
    writer.finalize().context("failed to finalize WAV")?;
    Ok(())
}
