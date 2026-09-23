//! DSP sanity checks: oscillators, envelope, filters, dynamics.

use autosynth::dsp::StereoFrame;
use autosynth::dsp::effects::compressor::{self, Compressor};
use autosynth::dsp::effects::gate::Gate;
use autosynth::dsp::effects::limiter::Limiter;
use autosynth::dsp::effects::{Effect, FxCtx};
use autosynth::dsp::envelope::Adsr;
use autosynth::dsp::filter::{FilterType, Ladder, Svf};
use autosynth::dsp::oscillator::{Osc, OscState};
use autosynth::dsp::waveform::Waveform;

const SR: f32 = 44_100.0;

fn rms(buf: &[f32]) -> f32 {
    (buf.iter().map(|s| s * s).sum::<f32>() / buf.len() as f32).sqrt()
}

fn osc(wave: Waveform, freq: f32, n: usize) -> Vec<f32> {
    let oscs = [Osc::new(wave)];
    let mut st = OscState::new();
    st.reset(&oscs);
    st.tune(&oscs, freq, &[0.0], 1.0 / SR);
    (0..n).map(|_| st.render(&oscs, &[1.0])).collect()
}

fn ctx() -> FxCtx {
    FxCtx { sample_rate: SR, bpm: 120.0, beat: 0.0, beats_per_sample: 120.0 / 60.0 / SR as f64 }
}

#[test]
fn sine_rms_is_correct() {
    assert!((rms(&osc(Waveform::Sine, 440.0, SR as usize)) - 0.707).abs() < 0.02);
}

#[test]
fn saw_is_band_limited_and_bounded() {
    let buf = osc(Waveform::Saw, 1000.0, 2_000);
    assert!(buf.iter().all(|s| s.is_finite() && s.abs() <= 1.5));
    assert!(rms(&buf) > 0.3);
}

#[test]
fn noise_is_broadband() {
    let buf = osc(Waveform::Noise, 440.0, 4_000);
    assert!(rms(&buf) > 0.4 && buf.iter().all(|s| s.abs() <= 1.0));
}

#[test]
fn unison_keeps_level_and_thickens() {
    let oscs = [Osc { wave: Waveform::Saw, unison: 7, spread: 0.3, phase: 0.0 }];
    let mut st = OscState::new();
    st.reset(&oscs);
    st.tune(&oscs, 220.0, &[0.0], 1.0 / SR);
    let buf: Vec<f32> = (0..SR as usize).map(|_| st.render(&oscs, &[1.0])).collect();
    let r = rms(&buf);
    assert!(r > 0.2 && r < 1.2, "1/sqrt(n) normalization keeps the stack near unit level (rms {r})");
}

#[test]
fn envelope_reaches_sustain_then_releases() {
    let mut env = Adsr::new();
    env.attack = 0.005;
    env.decay = 0.005;
    env.sustain = 0.5;
    env.release = 0.01;
    let dt = 1.0 / SR;
    env.note_on();
    let mut level = 0.0;
    for _ in 0..(0.05 * SR) as usize {
        level = env.next_sample(dt);
    }
    assert!((level - 0.5).abs() < 0.01);
    env.note_off();
    for _ in 0..(0.05 * SR) as usize {
        level = env.next_sample(dt);
    }
    assert!(level < 0.01 && !env.is_active());
}

#[test]
fn svf_lowpass_attenuates_high_frequencies() {
    let input = osc(Waveform::Sine, 6_000.0, 8_000);
    let mut f = Svf::new();
    let out: Vec<f32> = input.iter().map(|&x| f.process(x, 200.0, 0.0, FilterType::Lowpass, SR)).collect();
    assert!(rms(&out[2_000..]) < rms(&input[2_000..]) * 0.2);
}

#[test]
fn ladder_is_steep_and_stable_at_high_resonance() {
    let high = osc(Waveform::Sine, 8_000.0, 8_000);
    let low = osc(Waveform::Sine, 100.0, 8_000);
    let mut f = Ladder::default();
    let out_high: Vec<f32> = high.iter().map(|&x| f.process(x * 0.5, 500.0, 0.0, SR)).collect();
    let mut f = Ladder::default();
    let out_low: Vec<f32> = low.iter().map(|&x| f.process(x * 0.5, 500.0, 0.0, SR)).collect();
    assert!(rms(&out_high[2_000..]) < rms(&out_low[2_000..]) * 0.01, "24 dB/oct: 8 kHz is gone at 500 Hz");

    let saw = osc(Waveform::Saw, 110.0, 20_000);
    let mut f = Ladder::default();
    let out: Vec<f32> = saw.iter().map(|&x| f.process(x, 1_200.0, 0.98, SR)).collect();
    assert!(out.iter().all(|s| s.is_finite() && s.abs() < 4.0), "resonance stays bounded");
}

#[test]
fn compressor_reduces_loud_signals() {
    let mut c = Compressor::new();
    c.set(compressor::THRESHOLD, -20.0);
    c.set(compressor::RATIO, 4.0);
    let mut buf: Vec<StereoFrame> = osc(Waveform::Sine, 200.0, 8_000).iter().map(|&x| [x, x]).collect();
    c.process(&mut buf, &ctx());
    assert!(c.reduction_db() > 8.0, "a 0 dBFS sine sits ~17 dB over threshold at 4:1 (got {})", c.reduction_db());
}

#[test]
fn limiter_holds_the_ceiling() {
    let mut l = Limiter::new();
    let mut buf: Vec<StereoFrame> = osc(Waveform::Sine, 200.0, 8_000).iter().map(|&x| [x * 3.0, x * 3.0]).collect();
    l.process(&mut buf, &ctx());
    let ceiling = 10f32.powf(-0.3 / 20.0);
    assert!(buf.iter().all(|f| f[0].abs() <= ceiling + 1e-4));
}

#[test]
fn gate_chops_on_the_beat() {
    let mut g = Gate::new("x.");
    // Default grid is a 16th: open on even 16ths, closed on odd ones.
    let n = (SR * 0.5) as usize; // one beat at 120 BPM
    let mut buf: Vec<StereoFrame> = vec![[1.0, 1.0]; n];
    g.process(&mut buf, &ctx());
    let sixteenth = n / 4;
    let open = buf[sixteenth / 2][0];
    let closed = buf[sixteenth + sixteenth / 2][0];
    assert!(open > 0.9 && closed < 0.1, "open {open}, closed {closed}");
}
