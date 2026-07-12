use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};

/// Mono f32 audio data loaded from a WAV file.
pub struct SampleData {
    pub samples: Vec<f32>,
    pub sample_rate: f32,
}

impl SampleData {
    /// Load a WAV file, converting to mono f32 regardless of source format.
    pub fn load(path: &Path) -> Result<Self> {
        let reader =
            hound::WavReader::open(path).with_context(|| format!("failed to open {}", path.display()))?;
        let spec = reader.spec();
        let sample_rate = spec.sample_rate as f32;
        let channels = spec.channels as usize;

        let mono = match spec.sample_format {
            hound::SampleFormat::Int => {
                let bits = spec.bits_per_sample;
                let max = (1u32 << (bits - 1)) as f32;
                let all: Vec<f32> = reader
                    .into_samples::<i32>()
                    .map(|s| s.unwrap() as f32 / max)
                    .collect();
                mix_to_mono(&all, channels)
            }
            hound::SampleFormat::Float => {
                let all: Vec<f32> = reader
                    .into_samples::<f32>()
                    .map(|s| s.unwrap())
                    .collect();
                mix_to_mono(&all, channels)
            }
        };

        Ok(Self {
            samples: mono,
            sample_rate,
        })
    }
}

/// Mix interleaved multi-channel audio down to mono.
fn mix_to_mono(interleaved: &[f32], channels: usize) -> Vec<f32> {
    if channels == 1 {
        return interleaved.to_vec();
    }
    interleaved
        .chunks_exact(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect()
}

/// Caches loaded samples on the UI thread, handing out `Arc` for zero-copy
/// sharing with the audio thread.
pub struct SampleCache {
    cache: HashMap<PathBuf, Arc<SampleData>>,
}

impl SampleCache {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }

    /// Load a sample (or return cached). Returns `None` and logs on error.
    pub fn get(&mut self, path: &Path) -> Option<Arc<SampleData>> {
        if let Some(arc) = self.cache.get(path) {
            return Some(Arc::clone(arc));
        }
        match SampleData::load(path) {
            Ok(data) => {
                let arc = Arc::new(data);
                self.cache.insert(path.to_path_buf(), Arc::clone(&arc));
                Some(arc)
            }
            Err(e) => {
                tracing::error!("failed to load sample {}: {e:#}", path.display());
                None
            }
        }
    }
}
