//! Effect-chain presets — plain `fn(&mut Chain)`. Use one as an insert
//! (`t.fx(chains::dub)`), on a bus (`b.fx(chains::hall)`), or as the master
//! (`s.master(chains::glue)`); compose them by calling one from another.

use crate::live::chain_builder::Chain;
use crate::music::duration::{DurExt, N8};

/// Roomy stereo reverb, mixed in — for tracks routed wholly into a bus.
pub fn space(c: &mut Chain) {
    c.reverb().size(0.85).damp(0.4).mix(0.35).width(1.0);
}

/// A 100%-wet hall — for send buses.
pub fn hall(c: &mut Chain) {
    c.reverb().size(0.93).damp(0.3).mix(1.0);
}

/// Dotted ping-pong echoes into a small room.
pub fn dub(c: &mut Chain) {
    c.delay(N8.dotted()).feedback(0.55).mix(0.35).ping_pong();
    c.reverb().size(0.5).mix(0.15);
}

/// Bus glue: gentle compression, then a limiter.
pub fn glue(c: &mut Chain) {
    c.comp().threshold(-14.0).ratio(2.0).attack(0.02).release(0.2).makeup(2.0);
    c.limit();
}

/// Chorus plus mid/side widening.
pub fn wide(c: &mut Chain) {
    c.chorus().rate(0.3).depth(0.003).mix(0.3);
    c.width(1.4);
}

/// Crushed and dulled.
pub fn lofi(c: &mut Chain) {
    c.crush().bits(10.0).down(3.0).mix(0.5);
    c.lowpass(5000.0);
}

/// Warm saturation with a little wow.
pub fn tape(c: &mut Chain) {
    c.drive(1.5).saturate().mix(0.5);
    c.chorus().rate(0.5).depth(0.001).mix(0.3);
}
