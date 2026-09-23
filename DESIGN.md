# autosynth — Design

autosynth is a Rust live-coding synthesizer where plain functions *are* the music. A
track is `fn bass(t: &mut Track)`, a bus is `fn space(b: &mut Bus)`, an effect chain is
`fn dub(c: &mut Chain)`, an instrument preset is `fn acid() -> Synth`, and an automation
macro is `fn energy() -> Signal`. You edit them while the set plays: sound changes are
heard immediately, pattern changes land on the next loop boundary.

## API tour

```rust
use autosynth::prelude::*;

fn main() -> anyhow::Result<()> {
    autosynth::live(scene) // `-- --render 32 out.wav` bounces offline instead
}

const INTRO: Section = Section::start(16);
const BUILD: Section = INTRO.then(16);
const DROP: Section = BUILD.then(32);

/// One macro drives the whole arrangement.
fn energy() -> Signal {
    curve().hold(INTRO, 0.2).rise(BUILD, 0.2, 1.0).ease().hold(DROP, 1.0).signal().max(knob(1))
}

fn scene(s: &mut Scene) {
    s.tempo(124.0);
    s.key(A2, MINOR);
    s.track(kick);
    s.track(bass);
    s.track(pads);
    s.bus(space);
    s.master(chains::glue);
    // s.jump(DROP);   // live arrangement, on the next bar
    // s.hold(BUILD);  // loop a section while this line exists
}

fn kick(t: &mut Track) {
    t.synth(presets::kick());
    t.play(bars(1), |p| {
        p.hits(C1, "x... x... x... x...");
        p.every(8, |p| { p.at(3.5).hits(C1, "xx"); });
    });
}

fn bass(t: &mut Track) {
    t.synth(presets::acid().ladder(300.0 + env(0.0, 0.2) * vel() * energy().range(800.0, 5000.0), 0.8));
    t.duck(kick).depth(0.6).release(N4);
    t.play(bars(2), |p| {
        p.seq("1 1 8 1  b2 1 . 8  1 1 b7 1  5 . 8 1")
            .slide("..x. ..x. .... x...")
            .vel("X... ..X. X... ..X.");
    });
    t.fx(|c| {
        c.drive(3.0).saturate().mix(0.5);
        c.send(space, energy().range(0.0, 0.3));
    });
}

fn pads(t: &mut Track) {
    t.synth(presets::supersaw(7, 0.25).attack(1.5).release(3.0));
    t.to(space);
    t.play(bars(16), |p| { p.chords("i VI III VII", bars(4)).voice_lead(); });
}

fn space(b: &mut Bus) {
    b.fx(chains::space);
}
```

The examples (`simple`, `maya`, `into_jfk`, `progressive`, `acid`) are complete songs:
`just live progressive` plays one with hot reload, `just render acid 32` bounces one.

---

## 1. Layers

Five modules, strictly ordered — each depends only on those above it:

```
music/   pure vocabulary: notes, durations, keys, harmony, form, signals, mini-notation, Phrase
dsp/     pure processors: oscillators, envelopes, filters, effects
model/   declarative, diffable scene description: instruments, chains, tracks, buses
engine/  real time: transport, scheduler, voices, chains, buses, the render loop
live/    scene runtime: builders, hot-reload diffing, sample cache, MIDI, cpal
```

plus two libraries of plain functions on top: `presets` (instruments) and `chains`
(effect chains). `prelude` is the only re-export module.

Data flows one way. Builders produce `model` values; `live` diffs them against what the
engine already has; differences become `Command`s over one channel; the engine applies
them at musically correct times and renders.

Two threads. The **control thread** runs your scene function every 50 ms, loads samples,
compiles signals and builds DSP nodes. The **audio thread** owns the engine. The engine
renders into any `&mut [f32]`, which is also how offline rendering and the tests work.

## 2. The function is the name

Every Rust function item has a unique type, so `TypeId::of::<F>()` identifies a track,
bus or master chain with no registration and no strings. `type_name::<F>()` (the full
module path) is the engine key, so `a::bass` and `b::bass` never collide. Anywhere the API
refers to another node it takes the function itself: `t.to(space)`, `t.duck(kick)`,
`c.send(space, 0.3)`, `s.midi(lead)`.

## 3. What happens when you save

`live` runs `scene(&mut s); s.finish_frame();` every frame through subsecond's jump table.
Each `s.track(f)` / `s.bus(f)` / `s.master(f)` checks `f`'s hot-patched address; if the
code didn't change, the builder doesn't run and nothing is sent. If it did, the builder
runs (inside `catch_unwind` — a panic logs and keeps the old version playing) and its spec
is diffed by value:

| Change | Command | Takes effect |
|---|---|---|
| new track | `AddTrack` | next bar line |
| track line deleted | `RemoveTrack` | immediately |
| instrument | `SetInstrument` | immediately; voices survive unless voice count or source kind changed |
| gain / pan / mute | `SetMixer` | immediately (smoothed) |
| chain | `SetChain` (per-node reconcile) | immediately; unchanged-kind nodes keep their state |
| route / duck / key | `SetRoute` / `SetDuck` / `SetTrackKey` | immediately |
| pattern (builder re-ran) | `QueuePattern` | next loop boundary |
| bus set or any bus | `SetBuses` (ordered, reconciled) | immediately |
| master chain | `SetMaster` (reconciled) | immediately |
| `s.jump(S)` appears/changes | `Jump` | next bar line |
| `s.hold(S)` present/absent | `Hold` | immediately |

**Sound changes are immediate; timing changes are quantized.** You never choose — the kind
of change decides.

Because signals are data (§5), the only thing that can't be compared is the pattern
closure; it is re-queued whenever its builder re-ran, which by construction means its
code changed.

**Chain reconciliation.** `FxChain::diff(prev, new)` walks both chains by position: a node
of the same kind (same effect, same mode) becomes `Keep { params, enabled }` and retains
its DSP state; a parallel node with the same branch count recurses; anything else is a
freshly built node. DSP is always constructed on the control thread and shipped as a box.
The upshot: tweak a delay's feedback, toggle `.enabled(false)`, automate a reverb's size —
tails keep ringing. Only changing *what* the node is rebuilds it.

## 4. Time: engine beats and song beats

The engine counts **beats** (`f64`), never samples; the transport is the only component
that converts. A tempo change is one field write and every loop, note-off and automation
stays correct.

There are two clocks:

- **engine beat** — monotonic. Schedulers anchor loops to it, so loops never jump.
- **song beat** — `engine beat + offset`. Signals, curves, sections and phrases read it.

`s.jump(DROP)` sets the offset at the next bar line; `s.hold(BUILD)` wraps the song beat
inside a section. The offset is always a whole number of bars, so running loops stay in
phase with the song grid across any jump. `Transport::song(g)` answers "what song beat is
engine beat `g`?" analytically — including a pending jump — so a phrase generated just
before a bar line already sees where the song will be.

### Scheduling

Each track owns a `Scheduler`: sorted note-ons in loop-local beats plus **note-off
obligations** at absolute beats, so a note that outlasts its loop still releases on time.
Only a *replaced* pattern (an edit) chokes held notes at the boundary.

`t.play(len, |p| ..)` stores a closure that re-runs for every loop: just before a boundary
falls inside a render block, the track generates the next loop and stages it. The closure
runs inside `catch_unwind`. Swing is applied where phrase output becomes note-ons.

New tracks launch on the next bar line, so adding a track mid-set lands on the grid.

## 5. Signals

Every knob takes `impl Into<Signal>`:

```rust
t.gain(0.8);
t.pan(sine(bars(4)).range(-0.3, 0.3));
t.gain(energy().range(0.0, 0.5));
c.reverb().mix(during(DROP) * 0.3 + 0.1);
c.lowpass(knob(21).rangex(200.0, 12000.0));
t.gain(Signal::func(|c: Ctx| (c.beat * 0.1).sin() as f32 * 0.5 + 0.5)); // escape hatch
```

A `Signal` is an expression tree: constants; LFOs (`sine`/`tri`/`saw`/`square`, period in
beats); `curve()`s, `after`, `during` and `Section::ramp` over the form; `knob(cc)`;
seeded `noise`/`smooth_noise`; arithmetic with numbers on either side; and combinators
(`range`, `rangex`, `clamp`, `pow`, `steps`, `ease`, `bipolar`, `max`, `min`, `fast`,
`slow`, `shift`). Every source is unipolar `0..1`, so `.range(lo, hi)` means the same thing
everywhere.

A `Mod` is the same tree plus **per-voice** sources — `env(a, d)`, `adsr(a, d, s, r)`,
`vel()`, `key()` (note tracking), `rnd()` (per note), `lfo(hz)` — and only voice
parameters accept it. `Signal: Into<Mod>`, so global signals work inside voice
modulation; the reverse is a compile error: `t.gain(env(0.0, 0.1))` doesn't type-check.

Signals derive `PartialEq`, so they diff by value. A `func` compares by pointer identity.

**Compilation.** On the control thread a signal compiles to a `Program`: flat postfix ops
evaluated on a fixed stack — no allocation, no recursion, safe on the audio thread.
Constant expressions fold. For a `Mod`, compilation also allocates the voice's envelope
and LFO slots in a `VoiceLayout` (up to four of each, deduplicated), so identical
`env(0.0, 0.2)`s in cutoff and pitch share one envelope.

**Rates.** Track, bus and effect parameters evaluate once per 64-frame control block and
feed one-pole smoothers. Voice programs evaluate every 16 samples per voice; cutoff and
gain ramp linearly across the block.

## 6. Instruments

Instruments are values built by chaining:

```rust
Synth::new()
    .osc(Saw.unison(7, 0.2))
    .osc(Square.level(0.3).semis(-12.0))
    .ladder(300.0 + env(0.0, 0.25) * vel() * 4000.0, 0.8)
    .adsr(0.002, 0.3, 0.6, 0.05)
    .pitch(lfo(5.0).bipolar() * 0.1)
    .mono().glide(0.05)
```

and presets are functions returning one (`presets::supersaw`, `acid`, `pluck`, `pad`,
`sub`, `reese`, `kick`, `snare`, `hat`), so tweaking a preset is chaining more calls.
`Sampler::new(path).root(C4)` plays a pitched sample; `t.slot(path)` builds a drum kit
(slots play to the end of the sample; `Sampler::kit()` shapes the kit voice).

One `Voice` type serves both: source (unison oscillators or a sample playhead) → optional
filter (SVF lowpass/highpass/bandpass/notch or the ZDF ladder) → amp ADSR × gain (default
`vel()`). Voices read the instrument's compiled programs; a hot edit swaps the programs and
every sounding voice hears it on its next modulation block.

**Mono, glide, slide.** A mono instrument plays overlapping notes legato: the pitch glides
instead of the envelopes retriggering when the synth has a glide time, the retrigger mode
is `Legato`, or the previous note *slides* (`.slide("..x.")` holds its gate into the next
note — the 303 idiom). Slides glide even on a synth without a glide time.

**Per-note locks.** Lanes like `.cutoff("800 1.2k . 400")`, `.res(..)`, `.decay(..)` and
`.gain(..)` override that voice parameter for individual notes, Elektron-style.

Retrigger modes: `Hard` resets envelope, phase and filter (percussive); `Soft` restarts
the attack from the current level (the default, click-free); `Legato` only re-pitches.

## 7. Patterns

`Phrase` is the note container a `play` closure fills. Notes land at a **cursor** on a
**grid** (a 16th by default):

```rust
p.note(E2, N2);                       // place, cursor advances
p.deg(5, N8).vel(0.6).step(N8.dotted());
p.seq("1 . b3 5 [8 7] <5 6> _ .");    // degrees in the key, one token per grid step
p.hits(kick, "x... x.x. ..x. X...");  // drum grid, X accents
p.chords("i VI III VII", bars(1)).voice_lead().spread(1);
p.arp("i7", Arp::UpDown, N16, bars(1));
p.euclid(hat, 5, 16);
```

**Mini-notation** is fixed-grid: each top-level token is one step, so a string's length in
steps is its length in time. `[a b]` subdivides a step, `[a,b]` stacks, `<a b>` alternates
by loop, `_` ties, `.`/`~` rest, `a*3` repeats, `a?` is a seeded coin flip, `|` is a
visual bar line. Degree atoms (`1`, `b3`, `#4`, `-2`, `8`) resolve against the key late,
at phrase time; note names start uppercase (`Eb2`); `chords` reads roman numerals (case
sets quality; `7 maj7 sus2 sus4 add9 ° + ø`, `b`/`#` roots, `/n` inversions). A bad string
logs its column and places nothing.

**Lanes** set per-note values on whatever was just placed: a number, a `Signal` sampled at
each note's song position (`.vel(energy())`), or a lane string cycled over the notes —
numbers when it contains digits, otherwise one character per note (`X` accent, `x` normal,
`g` ghost, `.` unchanged). Stacked notes share a lane step.

**Control and transforms.** `p.every(4, f)` runs on every 4th loop (fills land at the end
of the phrase), `p.sometimes(0.3, f)`, `p.during(DROP)`, `p.after(BUILD)`,
`p.progress(BUILD)`, `p.root(5)` (degrees relative to a chord root), and whole-phrase
`degrade`, `humanize`, `rotate`, `rev`, `transpose`.

**Determinism.** `p.cycle` counts loops on the *song* grid; the phrase RNG is seeded from
the track key mixed with the cycle. Loops re-roll live, jumping back to a section replays
its randomness, and two offline renders are byte-identical.

## 8. Chains, buses, sidechain

`t.fx(|c| ..)` appends to a track's chain; so does `t.fx(chains::dub)`, because a chain is
just `fn(&mut Chain)`. Effects: `delay` (tempo-synced, follows tempo live, glides on time
changes, ping-pong), `reverb` (Freeverb), `chorus`, `drive` (five shapes), `lowpass` /
`highpass` / `bandpass` / `ladder`, `eq` (three RBJ bands), `comp`, `gate` (trance gate
locked to song position), `phaser`, `crush`, `width`, `limit`, and `custom(..)` for your
own `CustomEffect`. Every parameter is a signal. `c.parallel(a, b)` sums two branches;
`c.send(bus, amount)` taps the signal into a bus and continues.

Each effect lives in one file in `dsp/effects/` with its parameter table (`PARAMS`) next to
its DSP; adding one is that file, one `FxKind` variant, and one builder method.

**Signal flow per track:** instrument → gain × duck → pan → chain (sends tap here, so they
follow the fader) → its route (master or a bus).

**Buses** (`s.bus(space)`, `fn space(b: &mut Bus)`) have a chain, then a fader, then a
route. The scene orders buses so each precedes everything it feeds; cycles are logged and
broken.

**Sidechain** is trigger-based: `t.duck(kick).depth(0.6).release(N4)` dips the track's gain
on every note-on of `kick` (3 ms attack, quadratic recovery over `release` beats). It
reads note events, not audio, so it is exact, deterministic and free of ordering
dependencies — and works even when the source is muted.

## 9. The render block

The engine renders in 64-frame control blocks:

```
1. transport: bake pending jumps / hold wraps
2. every track: evaluate gain/pan, apply queued patterns, regenerate, collect events
3. forward each duck source's note-on offsets to its ducking tracks
4. every track: render voices (split at each event's sample offset), gain × duck, pan,
   chain (sends into bus inputs), add into its route
5. buses in order: chain, fader, add into their route
6. master chain, then a final soft limiter as a safety net
```

Events land on exact samples regardless of buffer size, which is what makes offline
renders bit-identical to live output. Tracks are kept in insertion order so summation
order — and so the output — is deterministic.

## 10. What runs on the audio thread

Steady-state rendering allocates nothing: voices, scratch and bus buffers are
preallocated, and programs evaluate on a fixed stack. Knowingly bent rules:

- **Commands** arrive over `std::sync::mpsc` and are applied (old state dropped) at the
  top of the callback. Edits may allocate for a few microseconds; playback doesn't.
- **Pattern closures** run in the callback once per loop boundary (inside
  `catch_unwind`), including parsing their mini-notation strings.
- **`Signal::func` closures** run at control rate; keep them arithmetic.

Samples are decoded on the control thread and shared as `Arc<SampleData>`.

MIDI (`midi` feature): notes go to the track `s.midi(track)` armed; control changes become
`knob(cc)` values.

## 11. Offline render and testing

`autosynth::render(scene, bars, "out.wav")`, or any example with
`-- --render <bars> <file>`, runs the same engine without an audio device. That is also
the test strategy:

- **music**: signal evaluation against closed forms, curves and sections, mini-notation
  parsing, roman numerals and voice leading, phrase placement, lanes, slides, seeded RNG.
- **dsp**: oscillator levels, unison normalization, envelope shape, SVF and ladder
  response and stability, compressor, limiter, gate.
- **engine**: sample-exact onsets across tempo changes and swing, filter envelopes,
  velocity modulation, slide/glide pitch, per-note locks, duck depth and recovery, sends,
  bus routing, song-position jumps, chain reconciliation keeping delay tails.
- **render**: a full scene (presets, buses, duck, curves, randomness) renders twice to
  byte-identical WAVs.
