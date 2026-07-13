//! DSP sanity checks: oscillator levels, envelope shape, filter attenuation.

use autosynth::dsp::envelope::Adsr;
use autosynth::dsp::filter::{FilterType, Svf};
use autosynth::dsp::oscillator::{OscPhases, Oscillator};
use autosynth::dsp::waveform::Waveform;

fn rms(buf: &[f32]) -> f32 {
    (buf.iter().map(|s| s * s).sum::<f32>() / buf.len() as f32).sqrt()
}

#[test]
fn sine_rms_is_correct() {
    let cfg = vec![Oscillator::new(Waveform::Sine).level(1.0)];
    let mut osc = OscPhases::new(&cfg);
    let sr = 44_100.0;
    let dt = 1.0 / sr;
    let buf: Vec<f32> = (0..sr as usize).map(|_| osc.render(&cfg, 440.0, dt)).collect();
    // A unit sine has RMS 1/sqrt(2) ≈ 0.707.
    assert!((rms(&buf) - 0.707).abs() < 0.02, "sine RMS was {}", rms(&buf));
}

#[test]
fn saw_is_band_limited_and_bounded() {
    let cfg = vec![Oscillator::new(Waveform::Saw).level(1.0)];
    let mut osc = OscPhases::new(&cfg);
    let dt = 1.0 / 44_100.0;
    let buf: Vec<f32> = (0..2_000).map(|_| osc.render(&cfg, 1000.0, dt)).collect();
    assert!(buf.iter().all(|s| s.is_finite() && s.abs() <= 1.5));
    assert!(rms(&buf) > 0.3, "saw should carry energy");
}

#[test]
fn noise_is_broadband() {
    let cfg = vec![Oscillator::new(Waveform::Noise).level(1.0)];
    let mut osc = OscPhases::new(&cfg);
    let dt = 1.0 / 44_100.0;
    let buf: Vec<f32> = (0..4_000).map(|_| osc.render(&cfg, 440.0, dt)).collect();
    assert!(rms(&buf) > 0.4 && buf.iter().all(|s| s.abs() <= 1.0));
}

#[test]
fn envelope_reaches_sustain_then_releases() {
    let mut env = Adsr::new();
    env.attack = 0.005;
    env.decay = 0.005;
    env.sustain = 0.5;
    env.release = 0.01;
    let dt = 1.0 / 44_100.0;
    env.note_on();
    // Run 50 ms — well past attack+decay.
    let mut level = 0.0;
    for _ in 0..(0.05 * 44_100.0) as usize {
        level = env.next_sample(dt);
    }
    assert!((level - 0.5).abs() < 0.01, "sustain level was {level}");

    env.note_off();
    for _ in 0..(0.05 * 44_100.0) as usize {
        level = env.next_sample(dt);
    }
    assert!(level < 0.01, "should have released to ~0, got {level}");
    assert!(!env.is_active());
}

#[test]
fn lowpass_attenuates_high_frequencies() {
    // Feed a 6 kHz sine through a 200 Hz lowpass; output energy must drop hard.
    let sr = 44_100.0;
    let dt = 1.0 / sr;
    let cfg = vec![Oscillator::new(Waveform::Sine).level(1.0)];
    let mut osc = OscPhases::new(&cfg);
    let mut filt = Svf::new();
    let mut input = Vec::new();
    let mut output = Vec::new();
    for _ in 0..8_000 {
        let x = osc.render(&cfg, 6_000.0, dt);
        input.push(x);
        output.push(filt.process(x, 200.0, 0.0, FilterType::Lowpass, sr));
    }
    // Ignore the first samples while the filter settles.
    assert!(
        rms(&output[2_000..]) < rms(&input[2_000..]) * 0.2,
        "lowpass should strongly attenuate 6 kHz"
    );
}
