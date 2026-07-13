# autosynth — Design Doc

**Goal.** A Rust live-coding synthesizer where plain, elegant Rust functions *are* the music.
Edit a function, save, and the sound updates — sound changes immediately, timing changes at
the next loop boundary. The API should feel inevitable: no ceremony, no engine plumbing
visible from user code.

This document reviews the current implementation (~5k lines), identifies what to keep, cut,
and rewrite, and lays out the target architecture.

---

## 1. What the current code gets right

These are the load-bearing ideas. They survive the redesign untouched or strengthened:

- **The function is the track.** `s.track(bass)` uses the function's `TypeId` as identity and
  its name as the display name (`scene.rs`). No registration, no string keys in user code.
  This is the signature move of the whole library — keep it.
- **`IntoVal` parameter polymorphism.** `t.cutoff(800.0)` and
  `t.cutoff(|c: Clock| 1000.0 + 300.0 * (c.beat * 0.1).sin())` through one method
  (`automation.rs`). Static value or clock-driven automation, decided by the type system.
  This *is* the automation API. Keep.
- **`every(beats, |p| …)` regenerating phrases.** The closure re-runs at each loop boundary,
  so `rand::` calls naturally re-roll per iteration, and `p.iteration` enables evolving
  patterns. Elegant, and exactly right for live coding. Keep.
- **Two-speed hot reload semantics.** Sound changes (patch, fx params, automations) apply
  immediately; timing changes (pattern, loop length) queue to the loop boundary
  (`engine.rs` `UpdateTrack`, `track.rs` `PendingTimingUpdate`). Musically correct. Keep the
  semantics; simplify the mechanism (§4.5).
- **Structural diffing to preserve DSP state.** Unchanged effects keep their delay buffers
  across reloads (`TrackSnapshot` in `scene.rs`). Right instinct; the implementation has
  holes (§2) but the principle stays.
- **Headless-capable engine.** `Engine::render(&mut [f32])` is already separable from cpal.
  This makes offline rendering and golden-file testing nearly free (§6, §7).
- **The music vocabulary.** `notes.rs`, `harmony.rs`, `duration.rs`, `pattern.rs` are pure
  data and pure functions. Cheap, useful, correct. Keep (minus aliases, §3).

## 2. Defects found (evidence for the rewrite)

Concrete bugs and structural flaws in the current code. Each is a symptom of an
architectural gap, cited where it matters for §4.

1. **Tempo changes break running music.** Loop lengths and event times are converted to
   *samples at launch time* using the tempo of that moment (`track.rs:227-231`,
   `score.rs:144-152`). `SetTempo` updates the session but no running player, and
   `ClipGenerator` captures its own stale `Tempo` copy. Worse, the global beat is recomputed
   from sample 0 with the *current* bpm (`track.rs:363`), so a tempo change teleports every
   clock. → transport rewrite (§4.1).
2. **NoteOffs past the loop end are silently dropped.** The player resets its index at the
   wrap, discarding any event scheduled beyond the boundary; `all_notes_off()` at every wrap
   (`track.rs:324`) papers over it by choking releases. → scheduler rewrite (§4.1).
3. **Automation runs at 16th-note granularity** (`track.rs:362-365`), so a cutoff sweep is a
   4-steps-per-beat staircase. → control-rate evaluation + smoothing (§4.2).
4. **`.enabled(false)` toggle is lost on hot reload.** The enabled flag lives outside
   `EffectConfig`, so the snapshot diff can't see it; when configs are equal, `send_update`
   omits `fx_enabled` entirely (`scene.rs:368-374`). → unified param model (§4.4).
5. **`polyphony()` changes are silently ignored on reload.** `send_update` discards the
   field (`scene.rs:352`) and `UpdateTrack` can't carry it.
6. **Track name collisions.** `short_name` keeps only the last path segment (`scene.rs:34`),
   so `a::bass` and `b::bass` map to the same engine track and fight over it.
7. **Per-sample everything.** The render loop iterates a `HashMap` of tracks per *sample*
   (`session.rs:106-115`); each track does event-dispatch checks, f64 beat math, and virtual
   fx calls per sample. → block-based rendering (§4.2).
8. **Audio-thread allocation.** Commands carry `String`/`Vec`/`Box` that are freed on the
   audio thread; `ClipGenerator::regenerate` runs user closures and allocates + sorts on the
   audio thread at every loop boundary. Workable, but unacknowledged. → RT rules (§4.6).
9. **Synth/Sampler are 80% copy-paste.** Voice allocation, stealing, ADSR propagation, LFO,
   filter, param clamping — duplicated across `synth.rs` and `sampler.rs`, then manually
   unified by an 11-method delegation enum (`track.rs:20-102`). → generic voice engine (§4.3).
10. **Patch state is smeared across voices.** ADSR fields are hand-copied into every voice in
    six places. A voice should *read* shared params, not own stale copies.
11. **Three parallel types per effect.** `Delay` / `DelayConfig` / `DelayBuilder`, times
    three effects — `effect_config.rs` is 411 lines of triplicated boilerplate wired by `u8`
    param slots. Adding one effect touches five files. → declarative params (§4.4).
12. **Dead and vestigial code.** `Track.gain` is never settable; `EventKind::Param` /
    `SetPatch` are never produced; `Score` exists only to wrap a `Vec` on its way to the
    engine; `Time::Seconds` is unused by the real API; `sound()/play()/fx()` delegators are
    identity calls; `Session::launch` auto-creates a default track that masks bugs.
13. **Naive oscillators alias badly.** Raw saw/square (`oscillator.rs:99-108`) produce
    audible aliasing in exactly the register leads live in. → polyBLEP (§4.3).
14. **Hard clipping as a mixing strategy.** Every synth clamps its own output and the session
    clamps the sum per sample. Loud mixes distort harshly by design. → mixer with soft
    limiter (§4.5).
15. **LFO is hardwired to cutoff only** (`synth.rs:223-226`). A "depth" knob that can only
    ever wobble the filter is a dead end. Fold pitch/amp targets into the automation system
    instead of growing a mod-matrix.

## 3. Cut list

Delete without replacement — each is indirection or duplication that fights the design:

| Cut | Why |
|---|---|
| `Score`, `Sequence` as public concepts | `Phrase` is the one note container; the engine consumes a plain sorted event list. |
| `Time::Seconds` (and the `Time` enum) | Everything is beats. `f32` beats everywhere; `b()` wrapper goes away too. |
| `EventKind::Param`, `EventKind::SetPatch` | Never constructed. Events are `NoteOn`/`NoteOff`. |
| `EngineHandle` as public API | Scene is *the* API. The handle's 18 mirror-methods become an internal command sender. |
| `Patch` builder methods (`patch.rs:52-115`) | `Patch` is plain data; `SceneTrack` is the only builder. |
| `Session::launch` auto-track-creation | Launch on a missing track is a bug, not a feature. |
| `Track.gain` field | Dead. Replaced by real mixer gain (§4.5). |
| `sound()` / `play()` / `fx()` delegators | Identity functions that promise granular reload they don't deliver. |
| Alias functions: `dot`, `tri`, `seq`, `double_dotted`, `bars_of` | One name per concept. |
| `add_track` / `add_track_with_polyphony` / `add_sampler_track` / `add_kit_track` (+ their four `Command` variants) | One `TrackSpec` struct, one `Command::AddTrack`. |
| Per-effect `PARAM_*: u8` slot constants | Subsumed by the param system (§4.4). |

The `live` feature flag splits into `hot-reload` (subsecond + dioxus-devtools) and `midi`
(midir) — they are unrelated capabilities.

## 4. Target architecture

Five layers, strictly ordered; each depends only on the layers above it in this list:

```
music/    pure vocabulary: notes, harmony, durations, Phrase & combinators
model/    declarative, diffable scene description: TrackSpec, PatchSpec, FxSpec, params
dsp/      pure per-block processors: oscillators, ADSR, SVF, effects (no track knowledge)
engine/   real-time: Transport, Scheduler, VoiceBank, Mixer, command loop
live/     Scene runtime: hot-reload diffing, sample cache, MIDI, cpal setup
```

The data flow is one-directional: user functions build `model` values → `live` diffs them
against the previous frame → diffs become commands → `engine` applies them at musically
correct times → `dsp` makes the samples. The user only ever touches `music` and the builder
surface of `live`.

### 4.1 Beat-native transport (rewrite)

The root cause of defect group 1–2 is that time is converted to samples too early. Invert it:

- The engine owns a `Transport { beat: f64, bpm: f64 }`. Each block advances
  `beat += frames * bpm / (60 * sample_rate)`. Tempo changes take effect at the next block —
  every downstream consumer is automatically correct, including mid-flight loops.
- All scheduling is in beats: events are `(beat: f64, NoteOn/NoteOff)`, loops are
  `loop_len: f64` beats, boundaries are fractional-beat positions resolved to sample offsets
  *within the current block*.
- The per-track scheduler is a sorted event list with a cursor, but note-offs are tracked as
  *obligations*: a NoteOn schedules its own off, and offs survive loop wraps instead of being
  dropped. `all_notes_off` at the boundary remains only as the intentional behavior for
  *replaced* patterns, not a leak-plugging default.
- `Clock { beat, local, iteration }` is computed from the transport, not reverse-engineered
  from sample counts.

### 4.2 Block-based rendering with control-rate automation (rewrite)

Replace the per-sample outer loop (`session.rs:102-136`) with:

- `render(block)` per track: split the block at event beats (sample-accurate), render
  sub-slices. Within a sub-slice nothing changes, so the voice loop is tight and
  vectorizable.
- Automation closures evaluate once per control period (64 samples, not 16th notes), writing
  *targets*; audible params (gain, cutoff) glide to targets over the control period
  (one-pole or linear dezipper). This fixes stair-stepping and clicks in one mechanism.
- Tracks render into a scratch stereo buffer; the mixer sums buffers. HashMap iteration
  happens once per block, not once per sample.

### 4.3 One voice engine, generic over the sound source (rewrite)

Collapse `Synth` + `Sampler` + `SoundSource` into:

```rust
trait Source: Send {                    // the only thing that differs per instrument
    fn trigger(&mut self, note: u8);
    fn render(&mut self, out: &mut [f32]);   // pre-env, pre-filter
    fn finished(&self) -> bool;              // sampler: buffer exhausted
}

struct Voice<S: Source> { source: S, env: Adsr, filter: Svf, note: u8, vel: f32, … }
struct VoiceBank<S: Source> { voices: Vec<Voice<S>>, patch: Arc-or-shared Patch, … }
```

`OscBank` (the synth source) and `SamplePlayhead` / `KitPlayhead` implement `Source`.
Voice allocation, stealing, retrigger modes, envelope, filter, and param handling exist
*once*. The track holds `VoiceBank<OscBank>` or `VoiceBank<SamplePlayhead>` behind one small
enum — the enum delegates 3 methods, not 11, because everything shared lives in `VoiceBank`.

While rewriting: oscillators get polyBLEP band-limiting (saw/square; ~15 lines), and a
`Noise` waveform is added — both trivial once the render is block-based.

### 4.4 A single parameter system (rewrite; deletes `effect_config.rs`)

Everything automatable is a `Param`: a typed id + range + smoothing policy. Tracks expose a
flat param table (synth params, per-osc params, per-effect params, fx-enabled, mixer
gain/pan). Consequences:

- An automation is `(ParamId, Box<FnMut(Clock) -> f32>)` — one representation for what is
  currently seven `AutoCmd` variants plus per-effect slot plumbing.
- Effects declare their params once, declaratively (a small `params!` macro or a
  `Params` trait with an array of descriptors). The config-for-diffing and the
  builder-with-`IntoVal` are *derived* from that single declaration. Adding an effect means
  writing its DSP and its param list — nothing else.
- Fx-enabled becomes a param like any other, which fixes defect 4 structurally: it's in the
  diffable model, so a static toggle is a visible change.
- Param ids are stable handles (track-scoped), not positional `fx_index`/`u8` slots, so
  reordering effects can't cross wires.

### 4.5 Declarative model + simpler diffing (simplify)

`SceneTrack` (the builder) produces a `TrackSpec`:

```rust
struct TrackSpec {
    source: SourceSpec,          // Synth(PatchSpec) | Sample{path, root} | Kit{slots}
    params: ParamValues,         // every static param value, diffable
    fx: Vec<FxSpec>,             // kind + param values, diffable
    polyphony: usize,
    loop_len: Option<f64>,
    pattern: Option<PatternFn>,  // closures: compared by HotFn ptr only
    automations: Vec<(ParamId, AutomationFn)>,
}
```

The Scene keeps `prev: HashMap<TrackId, TrackSpec>` and diffs whole specs. The rules stay
what they are today — new/removed tracks tear up/down; source changes rebuild; param/fx
changes apply now; pattern/loop changes queue to the boundary — but they fall out of *one*
diff over *one* complete description, instead of ptr-tracking + partial snapshots + special
cases. Because the spec is complete (params, enabled flags, polyphony all in it), defects
4–5 can't recur. Track names use the full module path with a short display name, fixing
defect 6.

The mixer joins the model here: per-track `gain` / `pan` / `mute` become spec fields (and
params, so they're automatable), tracks sum on a master bus with a soft-clip limiter — the
per-synth and per-session hard clamps are deleted.

### 4.6 Real-time discipline (harden)

Rules the engine crate enforces by construction:

- Commands travel over a bounded SPSC ring buffer; anything heap-allocated that the audio
  thread replaces (old fx boxes, old event lists) is sent *back* over a return ring and
  dropped on the control thread. No `malloc`/`free` in the callback.
- Pattern regeneration moves off the audio thread: the engine publishes "boundary for track T
  at beat B is approaching" one control-period early; the Scene thread runs the user closure
  and ships the event list; the engine swaps it in at the boundary. User code (with its
  `rand`, allocation, and possible panics) never runs in the callback. A panic in a pattern
  closure logs and keeps the previous loop playing — live sets don't stop.
- Voices, event lists, and scratch buffers are preallocated at track creation.

## 5. Necessary features

What "complete" means for this instrument. Roughly ordered; ★ = doesn't exist today.

**Sound**
- Subtractive synth: N oscillators (polyBLEP ★), detune/level/phase, ADSR, SVF, noise ★.
- Sampler: pitched one-shot + kit slots (exists, gets the shared voice engine).
- Effects: delay, chorus, distortion (exist) + **reverb ★** (the most-missed live-coding
  effect; a Freeverb/Dattorro is fine) + compressor ★ (later).
- Mixer ★: per-track gain/pan/mute, groups with shared fx (from `TODO.md`), master limiter.

**Time**
- Beat-native transport ★; tempo changes that just work ★.
- Quantized launch ★: new/changed tracks start at the next bar, not "wherever the ring buffer
  was" — this is `TODO.md`'s "queue restarted track" item.
- Swing/groove ★ (a per-track timing warp applied at schedule time — cheap once beats are
  native).

**Language** (all pure, all in `music/`)
- Existing: note constants, `note("C#4")`, scales/chords/degrees, `euclidean`, `arp`,
  `steps` mini-notation, phrase combinators (`then`, `layer`, `repeat`, `transpose`).
- `Clock` shape helpers ★: `c.phase(16.0)`, `c.ramp(a, b, len)`, `c.sin(len)` — the
  automation closures in every example hand-roll these.

**Live**
- Hot reload with the two-speed semantics (exists; rebuilt on §4.5).
- MIDI note input routed by `s.midi(track)` (exists); MIDI CC → param mapping ★.
- Offline render ★: `autosynth::render(scene, bars, "out.wav")` — same engine, no cpal.
  This is also the testing story (§7).

**Explicitly not features** (for now): arrangement timelines, plugin hosting, GUI
(`TODO.md`'s ratatui scope-view is a separate binary if it ever happens), mod matrix.

## 6. Target user code

The API barely moves — that's the point. Additions are marked:

```rust
fn scene(s: &mut Scene) {
    s.tempo(122.0);
    s.track(bass);
    s.track(drums);
    s.group("rhythm", &[drums, bass], |g| {     // NEW: group bus
        g.gain(0.9);
        g.reverb(|r| r.size(0.3).mix(0.2));     // NEW: reverb, bus fx
    });
}

fn bass(t: &mut Track) {
    t.osc(Waveform::Saw, 0.8);
    t.cutoff(|c: Clock| 400.0 + 300.0 * c.sin(8.0));  // NEW: Clock helpers
    t.pan(-0.2);                                       // NEW: mixer param
    t.every(8.0, |p| {
        p.note(0.0, E1, 0.9, 2.0);                     // beats are plain f32 — no b()
        p.steps(E1, "x..x..x.", 0.7);
    });
}
```

## 7. Testing strategy

The engine renders into a slice with no audio device — exploit it:

- **Timing tests**: schedule a phrase, render N blocks, assert note-on sample positions
  exactly (including across tempo changes and loop wraps — regression tests for defects 1–2).
- **DSP tests**: golden spectra/RMS for oscillators, filter, envelope, each effect.
- **Diff tests**: feed two `TrackSpec`s to the differ, assert the exact command sequence
  (regression tests for defects 4–6).
- **RT tests**: assert-no-alloc harness around the render callback in debug builds.

## 8. Migration plan

Each phase compiles, passes tests, and keeps the examples playing:

1. **Deletions + model extraction.** Apply the cut list (§3); introduce `TrackSpec` and the
   param table; rewrite `Scene` diffing on top (§4.4–4.5). Mostly moves & deletions; fixes
   defects 4–6 and 12.
2. **Transport.** Beat-native clock and scheduler (§4.1). Fixes defects 1–2. Add timing tests
   first — they define the contract.
3. **Voice engine + blocks.** `VoiceBank<S: Source>`, block rendering, control-rate
   automation with smoothing, polyBLEP (§4.2–4.3). Fixes defects 3, 7, 9, 10, 13.
4. **Mixer.** Track gain/pan, groups, master limiter, reverb (§4.5). Fixes defect 14.
5. **RT hardening.** SPSC rings, garbage return, off-thread pattern regen (§4.6). Fixes
   defect 8.
6. **Polish.** Quantized launch, swing, Clock helpers, MIDI CC map, offline render.
