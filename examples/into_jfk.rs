use anyhow::Result;
use autosynth::prelude::*;

fn main() -> Result<()> {
    autosynth::live(120.0, scene)
}

fn scene(s: &mut Scene) {
    s.track(chords);
    s.track(lead);
    s.track(bass);
    s.track(lead2);
}

fn lead2(t: &mut Track) {
    t.osc(Waveform::Triangle, 0.5);
    t.osc(Waveform::Saw, 0.5).phase_offset(0.5);
    t.osc(Waveform::Saw, 0.5).phase_offset(0.0).detune(0.005);
    t.gain(0.5);
    // t.filter_type(FilterType::Notch);
    // t.cutoff(|c: Clock| 500.0 + 500.0 * (c.beat * 0.025 * std::f32::consts::TAU).sin());
    // t.cutoff(|c: Clock| {
    //     let val = 5000.0 + 1000.0 * (c.beat * 0.025 * std::f32::consts::TAU).sin();
    //     println!("cutoff: {}", val);
    //     val
    // });
    // t.resonance(|c: Clock| 0.0 + 0.5 * (c.local / 16.0));

    // let deg = [1, 2, 1, 3, 6, 5, 4];

    t.every(16.0, |p| {
        let deg = (0..7)
            .map(|_| rand::random::<u8>() % 5 + 1)
            .map(|d| d as usize)
            .collect::<Vec<_>>();
        let dur = [e(), e(), e(), e(), e(), e(), e()];
        let delay = [
            dotted(e()),
            dotted(e()),
            e(),
            dotted(e()),
            dotted(e()),
            e(),
            dotted(e()),
            dotted(e()),
        ];

        let mut total_delay = 0.0;
        for (i, (d, d2)) in dur.iter().zip(delay.iter()).enumerate() {
            p.note(
                b(total_delay),
                degree(E4, &MAJOR, deg[i]),
                rand::random_range(0.6..0.8),
                *d,
            );
            total_delay += *d2;
        }
    });
}

fn bass(t: &mut Track) {
    t.osc(Waveform::Saw, 0.5);
    t.osc(Waveform::Sine, 0.5).detune(0.05);
    t.osc(Waveform::Sine, 0.5).detune(0.1).phase_offset(1.5);
    t.gain(0.7);
    t.attack(0.05);
    t.decay(0.05);
    t.sustain(0.9);

    t.cutoff(800.0);

    // distortion
    t.distortion(|d| {
        d.drive(15.0).saturate().mix(0.7).output(0.8);
    });

    t.every(16.0, |p| {
        p.note(b(0.0), E1, 0.6, w());
    })
}

fn lead(t: &mut Track) {
    t.osc(Waveform::Triangle, 1.0);
    t.gain(1.0);

    // t.chorus(|c| {
    //     c.rate(1.0).mix(0.5);
    // });
    t.distortion(|d| {
        d.drive(15.0).saturate().mix(0.7).output(0.3);
    });
    t.delay(|d| {
        d.beats(0.5).feedback(0.7).mix(0.7).ping_pong();
    });
    t.every(4.0, |p| {
        let iter = ((p.iteration / 4) % 3) as usize;
        p.note(b(0.0), degree(E4, &MAJOR, iter + 1), 0.6, s());
    })
}

fn chords(t: &mut Track) {
    t.osc(Waveform::Sine, 1.0);
    t.osc(Waveform::Saw, 0.5);
    t.gain(0.3);
    t.attack(1.0);
    t.sustain(0.9);

    // t.delay(|d| {
    //     d.beats(0.5).feedback(0.4).mix(0.4).ping_pong();
    // });

    t.every(w() * 4.0, |p| {
        p.chord(b(w() * 0.0), &diatonic_triad(E3, &MAJOR, 1), 0.5, w());
        p.chord(b(w() * 1.0), &diatonic_triad(E3, &MAJOR, 6), 0.7, w());
        p.chord(
            b(w() * 2.0),
            &diatonic_triad(E3, &MAJOR, if p.iteration % 3 == 2 { 7 } else { 2 }),
            0.6,
            w(),
        );
        p.chord(b(w() * 3.0), &diatonic_triad(E3, &MAJOR, 5), 0.5, w());
    });
}
