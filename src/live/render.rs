use std::path::Path;

use anyhow::{Context, Result};

use crate::engine::Engine;
use crate::live::scene::Scene;

/// Offline (non-real-time) render: run the same engine with no audio device and
/// write `bars` bars of stereo audio to a WAV file. This is also the testing
/// story — deterministic, device-free, sample-exact.
pub fn render(bpm: f32, scene_fn: fn(&mut Scene), bars: f32, path: impl AsRef<Path>) -> Result<()> {
    let sample_rate = 44_100.0_f32;
    let (mut engine, handle) = Engine::new(sample_rate, 2, bpm);

    // Evaluate the scene once; commands queue to the engine.
    let mut scene = Scene::new(bpm, sample_rate, handle);
    scene_fn(&mut scene);
    scene.finish_frame();
    drop(scene); // releases the handle; engine drains queued commands on render

    let total_beats = bars * 4.0;
    let seconds = total_beats * 60.0 / bpm;
    let frames = (seconds * sample_rate) as usize;

    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: sample_rate as u32,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer =
        hound::WavWriter::create(path.as_ref(), spec).with_context(|| "failed to create WAV")?;

    // Render in blocks, reusing one scratch buffer.
    const BLOCK: usize = 1024;
    let mut buf = vec![0.0f32; BLOCK * 2];
    let mut remaining = frames;
    while remaining > 0 {
        let n = remaining.min(BLOCK);
        let slice = &mut buf[..n * 2];
        slice.fill(0.0);
        engine.render(slice);
        for &s in slice.iter() {
            writer.write_sample(s)?;
        }
        remaining -= n;
    }
    writer.finalize().context("failed to finalize WAV")?;
    Ok(())
}
