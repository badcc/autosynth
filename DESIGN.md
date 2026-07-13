# autosynth — Architecture

autosynth is a Rust live-coding synthesizer where plain functions *are* the music. You
write a `fn bass(t: &mut Track)`, register it with `s.track(bass)`, and edit it while it
plays: sound changes (a filter cutoff, an effect mix) apply immediately; timing changes (a
new pattern, a different loop length) land at the next loop boundary, on the grid. No
registration, no string keys, no engine plumbing visible from user code.

This document explains how that works: how a function becomes a track, what actually
happens when you hit save, how time is represented so tempo changes don't derail running
loops, what runs on the audio thread, and the DSP underneath. It's written for a technical
user who knows some Rust — the goal is that nothing the instrument does surprises you.

```rust
fn main() -> Result<()> {
    autosynth::live(120.0, scene)
}

fn scene(s: &mut Scene) {
    s.track(bass);
    s.track(drums);
}

fn bass(t: &mut Track) {
    t.osc(Waveform::Saw, 0.8);
    t.cutoff(|c: Clock| 400.0 + 300.0 * c.sin(8.0));   // automation: a closure is a knob
    t.every(8.0, |p| {
        p.note(0.0, E1, 0.9, 2.0);
        p.steps(E1, "x..x..x.", 0.7);
    });
}
```

---

## 1. The five layers

The crate is five modules, strictly ordered — each depends only on the ones above it:

```
music/    pure vocabulary: notes, harmony, durations, Phrase, Clock
model/    declarative, diffable scene description: TrackSpec, PatchSpec, FxSpec, ParamId
dsp/      pure processors: oscillators, ADSR, SVF, effects — no track knowledge
engine/   real-time core: Transport, Scheduler, VoiceBank, Mixer, command loop
live/     scene runtime: hot-reload diffing, sample cache, MIDI, cpal setup
```

Data flows one way. Your track functions build `model` values (a `TrackSpec` per track);
the `live` layer diffs each spec against the previous frame; differences become commands;
the `engine` applies them at musically correct times; `dsp` crunches the samples. You only
ever touch `music` and the builder surface of `live` — everything below is the instrument.

Two threads. The **control thread** runs your scene function in a loop, loads samples,
builds effect boxes, and sends commands. The **audio thread** (the cpal callback) owns the
engine and renders. They meet at exactly one channel of `Command` values. The engine knows
nothing about hot-reload or cpal; it renders into any `&mut [f32]`, which is also how
offline rendering and the test suite work (§9).

Feature flags: `hot-reload` (subsecond + dioxus-devtools) and `midi` (midir) are
independent capabilities and can be compiled out separately.

## 2. The function is the track

The signature move of the library: `s.track(bass)` needs no name and no registration
because **the function's type is its identity**. In Rust, every function item has a unique
zero-sized type, so:

```rust
pub struct TrackId(TypeId);

pub fn track<F: Fn(&mut SceneTrack) + 'static>(&mut self, f: F) {
    let id = TrackId::of::<F>();            // TypeId::of::<F>() — the fn type IS the key
    let key = std::any::type_name::<F>();   // "my_set::bank_a::bass" — the engine key
    ...
}
```

Two facts fall out of this:

- **Identity is stable across edits.** Changing the body of `bass` doesn't change its
  type, so the engine track persists and hot reloads target it precisely.
- **Names are free and collision-proof.** `std::any::type_name` gives the full module
  path, so `bank_a::bass` and `bank_b::bass` are distinct engine tracks; only the last
  path segment is used for display.

The same trick powers `s.midi(bass)` (route MIDI input to that track) and
`g.track(bass)` inside group definitions — anywhere the API needs to refer to a track, it
takes the function itself.

## 3. What happens when you hit save

The `live(bpm, scene)` entry point builds the cpal stream, connects to the
dioxus-devtools server (which streams binary patches produced by subsecond's hot-patching
linker), and then runs a 20 Hz loop:

```rust
loop {
    subsecond::call(|| {
        scene_fn(&mut scene);     // your fn scene(s: &mut Scene)
        scene.finish_frame();
    });
    sleep(50ms);
}
```

`subsecond::call` dispatches through a jump table, so after a patch it invokes the *newest*
version of your scene function. Inside, every `s.track(f)` does a cheap check before doing
any real work:

```rust
let mut hot = subsecond::HotFn::current(f);
if self.ptrs.get(&id) == Some(&hot.ptr_address()) {
    return;   // this function wasn't recompiled — skip it entirely
}
```

Each track function's hot-patched code address is memoized. If a save didn't change
`bass`, its builder never re-runs and nothing is sent. If it did, the builder runs against
a fresh `SceneTrack`, producing a complete `TrackSpec` — a declarative description of the
track: source, patch, mixer settings, fx chain, polyphony, loop length, pattern closure,
automations.

The spec is then **diffed by value** against the previous frame's spec, and only the
differences become commands:

| Change detected | Action | When it applies |
|---|---|---|
| new track function | `AddTrack` | next bar (quantized launch, §5) |
| function removed from `scene` | `RemoveTrack` | immediately |
| `source` or `polyphony` changed | remove + re-add (voices rebuilt) | next bar |
| patch fields (osc/env/filter/LFO) | `SetPatch` | immediately, voices keep playing |
| gain / pan / mute | `SetMixer` | immediately (gain glides, §8) |
| fx params changed | `SetFx` (fresh DSP boxes) | immediately |
| **only** fx `enabled` flags changed | `SetFxEnabled` | immediately, **buffers preserved** |
| pattern / loop length | `QueuePattern` | next loop boundary |

The `SetFxEnabled` special case is worth noticing: toggling `.enabled(false)` on a delay
must not rebuild the delay, or you'd lose the echo tail ringing in its buffer. The diff
checks whether the fx chains are structurally identical (same kinds, same params) and, if
so, ships only the flag vector. More generally, the engine only rebuilds DSP state when
the diff proves it has to — unchanged effects keep their buffers, running voices survive
patch edits, and a source change (a different sample file, synth→sampler) is the only
thing that tears a track down.

Two things can't be value-compared: pattern closures and automation closures (they're
`Box<dyn FnMut ...>`). Patterns are handled by the pointer memoization above — the builder
only re-ran because *something* in the function changed, so the pattern is re-queued to
the next boundary. Automations are simply resent wholesale on every re-run; replacing a
`Vec` of boxed closures is cheap and always correct.

`finish_frame()` closes the loop: any track that was in the previous frame but wasn't
mentioned this frame gets removed (deleting the `s.track(...)` line *is* the delete
operation), group buses are reconciled as a whole set, and the MIDI routing target is
updated.

### Two speeds, on purpose

The table above encodes the library's central musical rule: **sound changes are
immediate, timing changes are quantized.** Tweaking a cutoff mid-phrase should be heard
*now*; swapping a drum pattern mid-bar would stumble, so it waits for the loop boundary.
You never opt into this — the kind of change determines the speed.

## 4. Time is beats, all the way down

Everything in the engine is scheduled in **beats** (`f64`), never in samples. The
transport is the only component that knows the conversion:

```rust
pub struct Transport { pub beat: f64, pub bpm: f64, pub sample_rate: f64 }
// each block: beat += frames * bpm / (60.0 * sample_rate)
```

Sample positions are derived at the last possible moment — "this note-on falls 23 samples
into the current block" — and never stored. The payoff: a tempo change is one field write.
Every running loop, every scheduled note-off, every automation clock is automatically
correct at the new tempo on the next block, because none of them ever cached a
sample-based time. (The previous generation of this engine converted beats to samples at
launch time; tempo changes teleported every clock. That entire bug class is gone by
construction.)

### The scheduler and note-off obligations

Each track owns a `Scheduler`: a sorted list of note-ons in loop-local beats, a cursor,
and the loop bookkeeping (`iter_base` = global beat where the current iteration started,
`iteration` counter). Per block, `collect(b0, b1)` emits every event in the beat range
`[b0, b1)`, wrapping the loop as many times as needed.

Note-offs are not stored in the pattern. When a note-on fires, the scheduler registers an
**obligation** — `(global_beat_of_release, note)` — in a pending list that is checked
every block, independent of the loop cursor:

```rust
out.push(Fired { beat: g, ev: NoteEv::On { note, vel } });
self.pending_offs.push((g + dur, note));   // survives loop wraps
```

So a note whose duration spills past the loop end still releases exactly on time, in the
next iteration. Held notes are only force-choked in one situation: when a *replaced*
pattern takes over at the boundary (an actual edit), the old pattern's obligations are
flushed as immediate note-offs so the outgoing loop doesn't ring over the new one. A mere
loop repeat never chokes anything.

### `every(beats, |p| …)` — regenerating patterns

A pattern is a closure, not data — `FnMut(&mut Phrase)` — and it re-runs for every loop
iteration. Just before a boundary falls inside the current render block, the track runs
the closure for the *upcoming* iteration and stages the result; the scheduler swaps it in
exactly at the boundary. Consequences:

- `rand::` calls inside the closure naturally re-roll each loop.
- `p.iteration` and `p.beat` (the loop count and the global beat of the iteration's start)
  let patterns evolve: every 4th loop gets a fill, intensity ramps across iterations.
- The closure runs inside `catch_unwind`. **A panic in your pattern logs an error and
  keeps the previous loop playing** — an out-of-bounds index in a scale lookup doesn't
  stop the set.

A regeneration that changes nothing structural keeps the iteration counter climbing; only
an actual loop-length change re-anchors the clock and resets the count.

### Quantized launch

Newly added tracks (and source-changed rebuilds) don't start "wherever the ring buffer
happened to be" — the engine starts them at the next bar (`ceil(beat / 4) * 4`). Adding a
track mid-set lands on the grid.

### Clock

Automation closures receive a `Clock { beat, local, iteration }` — global beat, position
within the current loop, and loop count. It's computed fresh from the transport and the
scheduler's anchors each control period, never reverse-engineered from a sample counter,
so it stays truthful across tempo changes and pattern swaps. `Clock` also carries the
shape helpers most automations want: `c.phase(len)` (rising 0→1 ramp over `len` beats),
`c.ramp(a, b, len)`, `c.sin(len)`, `c.tri(len)`.

## 5. The render path

The engine renders in **control blocks** of 64 frames (~1.5 ms at 44.1 kHz). Each cpal
callback drains the command channel, then loops over the buffer in ≤64-frame chunks:

```
per control block, per track:
  1. evaluate automation closures once (control rate), write param targets
  2. apply any queued timing swap whose boundary has arrived
  3. regenerate the upcoming pattern iteration if a boundary falls in this block
  4. collect note events in [b0, b1) from the scheduler
  5. render mono, splitting the block at each event's sample offset
  6. pan to stereo → fx chain → smoothed gain → sum into the bus
```

Step 5 is the sample-accuracy mechanism: the block is rendered in sub-slices between
events, with `note_on`/`note_off` applied at the exact sample offset
(`(event_beat - b0) / beats_per_sample`). Within a sub-slice nothing changes, so the inner
voice loop is tight. A note lands on the same sample whether the buffer is 64 or 4096
frames — which is also what makes offline renders bit-identical to live output.

The mix stage: ungrouped tracks sum straight into a master buffer. Grouped tracks sum
into a shared bus buffer first; the bus applies its own gain and fx chain (this is how
several tracks share one reverb), then sums into master. Finally the master passes through
a **soft limiter** — linear below 0.8, then a `tanh` squash toward ±1.0:

```rust
if |x| <= 0.8 { x } else { sign(x) * (0.8 + 0.2 * tanh((|x| - 0.8) / 0.2)) }
```

so a loud mix rounds over instead of hard-clipping. There are no other clamps anywhere in
the signal path — tracks are free to run hot into a bus.

Panning is equal-power (`cos`/`sin` of the pan angle), so a sound keeps constant perceived
loudness as it moves across the field.

## 6. One voice engine, generic over the sound source

Synth and sampler are the same machine. The only thing that differs between them is how a
triggered note turns into a raw mono signal, and that difference is one trait:

```rust
pub trait Source: Send + Sized {
    type Cfg: Send;                                  // shared config: osc list / sample map
    fn new(sample_rate: f32, cfg: &Self::Cfg) -> Self;
    fn trigger(&mut self, note: u8, cfg: &Self::Cfg);
    fn render(&mut self, cfg: &Self::Cfg, dt: f32) -> f32;   // pre-env, pre-filter
    fn finished(&self) -> bool;                      // sampler: buffer exhausted
    fn reset(&mut self, cfg: &Self::Cfg);
}
```

`VoiceBank<S: Source>` owns everything shared: the voice pool, allocation and stealing
(retrigger same-note voices first, otherwise steal the least-recently-used), the ADSR, the
per-voice filter, the LFO, and the parameter set. The two sources are small:

- `OscBank` — per-voice oscillator *phases* only. The oscillator configs live in the
  bank's shared `Cfg`, read at render time.
- `SamplePlayhead` — a fractional position and rate into an `Arc<SampleData>`. Pitched
  mode derives the rate from the note's distance to the root (`2^(semis/12)`, times the
  file/engine sample-rate ratio); kit mode looks the note up in a per-note sample map.
  Playback is linearly interpolated; when the buffer runs out, `finished()` flips and the
  bank releases the envelope automatically.

The associated `Cfg` type is the important design decision: **voices read shared state,
they don't own copies.** A hot-reload that changes an oscillator level writes one field in
the bank's `Cfg` and every sounding voice hears it on its next sample — there is no "walk
all voices and update their copies" code to get wrong, and no stale-copy bugs.

The track-facing wrapper is a two-variant enum (`Instrument::Synth(VoiceBank<OscBank>)` /
`Sampler(VoiceBank<SamplePlayhead>)`) that forwards a handful of methods. It stays small
precisely because everything interesting lives in the generic bank.

Retrigger modes (`Hard` / `Soft` / `Legato`) are decided by the envelope: `Hard` resets
envelope, phase, and filter (percussive, clicks by design); `Soft` restarts the attack
from the current level with phase and filter untouched (no discontinuity — the default);
`Legato` doesn't retrigger at all while the note-on phase is still running, just re-pitches
— the classic mono-synth feel.

## 7. Parameters and automation

Every knob in the API accepts either a number or a closure:

```rust
t.cutoff(800.0);
t.cutoff(|c: Clock| 1000.0 + 300.0 * (c.beat * 0.1).sin());
```

One method, both behaviors — resolved by the type system through `IntoVal`:

```rust
pub enum Val<T> { Fixed(T), Fn(Box<dyn FnMut(Clock) -> T + Send>) }

impl IntoVal<f32> for f32 { ... }                                  // literal → Fixed
impl<F: FnMut(Clock) -> f32 + Send + 'static> IntoVal<f32> for F { ... }  // closure → Fn
```

(The two impls don't overlap because `f32` doesn't implement `FnMut`.) A `Fixed` value
lands in the diffable spec. An `Fn` is evaluated once at `Clock::ZERO` to seed the spec —
so the diff still sees a sensible static value — and the closure is recorded as an
automation.

An automation is `(ParamId, Box<dyn FnMut(Clock) -> f32>)`. `ParamId` is one enum naming
everything automatable on a track — envelope and filter params, per-oscillator level and
detune, mixer gain and pan, any effect parameter (`Fx { index, slot }`), even effect
enable. One representation covers the synth, the sampler, the mixer, and the fx chain;
the same `IntoVal` sugar works inside effect builders
(`d.feedback(|c: Clock| ...)`).

**Control rate + smoothing.** Automation closures run once per 64-frame control period,
not per sample — cheap enough that a handful of closures per track is free. That alone
would produce 1.5 ms staircases on audible parameters, so the params that click get a
one-pole smoother (`Smoothed`): the closure writes a *target*, and the audible value
glides toward it per sample with an ~8 ms time constant. Cutoff and gains are smoothed;
timbre params that only matter at trigger time (envelope times, resonance) apply directly.
The same mechanism dezippers hot-reload edits — dragging a value in your editor and saving
repeatedly sounds like turning a knob, not a zipper.

## 8. DSP notes

All of `dsp/` is engine-agnostic: plain structs processing samples, individually testable.

- **Oscillators** — sine, triangle, saw, square, noise. Saw and square are band-limited
  with **polyBLEP**: the naive waveform plus a two-sample polynomial correction spliced in
  around each discontinuity, which cancels the aliasing that makes naive digital saws
  harsh in exactly the register leads live in. It's ~15 lines and costs two branches per
  sample. Noise is a xorshift32 PRNG per voice — no allocation, no global RNG lock.
- **Filter** — the Cytomic/Andrew Simper **state-variable filter**: two integrator states,
  coefficients derived per sample from cutoff and resonance via the tan-prewarp. Chosen
  because it stays numerically stable under fast modulation (LFO + automation + smoothing
  all drive cutoff), and one topology yields lowpass/highpass/bandpass/notch from the same
  two states.
- **Envelope** — linear ADSR with the retrigger behavior of §6. Release always ramps from
  the level at note-off, so releasing mid-attack doesn't jump.
- **Effects** — delay (tempo-synced via `.beats(0.5)` or free-running, with a ping-pong
  mode), chorus (modulated delay line), distortion (five waveshaping modes with bias, tone,
  and output gain), and a Freeverb-style reverb (eight damped comb filters into four
  allpass diffusers per channel, the classic tunings scaled to the engine sample rate,
  with a stereo-width control that cross-blends the wet channels). Each effect is a
  per-frame stereo processor with numbered parameter slots for automation; its diffable
  config and the mapping from config to DSP live in one place (`model/fx.rs`), so adding
  an effect means writing its DSP and its config — nothing else.

## 9. What runs on the audio thread

The honest inventory. Per control block, the callback does: advance the transport,
evaluate automation closures, collect scheduler events, run voices and effects, and mix —
all against **preallocated** buffers (each track's mono scratch and event list, the
engine's master and group buffers are allocated at creation and reused; steady-state
rendering allocates nothing).

Three things intentionally bend strict real-time rules, with eyes open:

- **Commands arrive over `std::sync::mpsc`** and are applied (and their old state dropped)
  at the top of the callback. Applying an edit can allocate and free — the deal is that
  *edits* may cost a few microseconds; steady-state playback doesn't. For a single-user
  live instrument pushing a handful of commands per save, this is inaudible in practice.
- **Pattern closures run in the callback**, once per loop boundary. They're user code, so
  they're wrapped in `catch_unwind` — a panic keeps the previous loop playing (§4). A
  pathologically slow pattern closure could still overrun the buffer; that's the current
  trade for regeneration that's exactly boundary-accurate.
- **Automation closures run in the callback** at control rate. They should be arithmetic
  on `Clock`; the `Clock` helpers exist so they can be one-liners.

Sample loading never touches the audio thread: WAV files are decoded to mono `f32` on the
control thread, cached, and shared as `Arc<SampleData>` — a kit re-using the same file
across slots shares one buffer, and shipping a sample to the engine is a pointer copy.

MIDI input (first available port, `midi` feature) is similarly thin: the midir callback
translates note-on/off into commands routed to whichever track `s.midi(track)` armed —
same channel, same quantization-free immediate path as everything else that's
sound-speed.

## 10. Offline render and testing

Because the engine renders into a plain slice, running it without an audio device is
trivial — and it's both a feature and the test strategy:

```rust
autosynth::render(120.0, scene, 16.0, "out.wav")?;   // 16 bars, same engine, no cpal
```

The scene function is evaluated once, commands drain on the first render call, and blocks
are written straight to a WAV. Output is deterministic and sample-exact, which makes the
interesting properties assertable in ordinary `cargo test`:

- **Timing**: render N blocks, assert the exact sample position of every note-on and
  note-off — including across tempo changes, loop wraps, and boundary-queued pattern
  swaps. The beat-native transport's contract is pinned by these tests.
- **Diffing**: build two `TrackSpec`s and assert what the differ sees — e.g. that an
  fx-enable toggle is a visible, enable-only change (buffers preserved, no rebuild).
- **DSP**: golden RMS/spectral checks on oscillators, filter, envelope, and effects as
  pure functions.

The result is a live instrument whose whole audible behavior — scheduling, hot-reload
semantics, DSP — is exercised headlessly, byte-for-byte, in CI.
