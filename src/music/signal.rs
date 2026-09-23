//! Signals: every knob's value, as data.
//!
//! A [`Signal`] is a small expression tree over *song time* — constants, LFO
//! shapes, section curves, MIDI knobs, arithmetic. A [`Mod`] is the same tree
//! but may also read *per-voice* sources (envelopes, velocity, key tracking,
//! voice LFOs), so it can only drive voice parameters. The split is enforced by
//! the type system: `t.gain(env(0.0, 0.2))` does not compile.
//!
//! Because signals are data they diff by value (a hot reload only resends what
//! changed), they can be displayed, and they compile to a flat [`Program`] that
//! the audio thread evaluates without allocation or recursion.
//!
//! Every source is unipolar `0..1` (like Strudel), so `.range(lo, hi)` means the
//! same thing on an LFO, an envelope, a knob or a curve. Use `.bipolar()` for
//! `-1..1`.

use std::f64::consts::TAU;
use std::fmt;
use std::ops::{Add, Div, Mul, Neg, Sub};
use std::sync::Arc;

use crate::music::form::Section;

/// Per-voice modulation envelopes a single instrument may use.
pub const MAX_ENVS: usize = 4;
/// Per-voice LFOs a single instrument may use.
pub const MAX_LFOS: usize = 4;
/// Evaluation stack depth. Left-leaning arithmetic uses two slots, so this is
/// far more than any hand-written expression needs.
const STACK: usize = 32;
/// Nesting depth of `fast`/`slow`/`shift` time transforms.
const TIME_STACK: usize = 8;

// ── Expression tree ──

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Wave {
    Sine,
    Tri,
    Saw,
    Square,
}

impl Wave {
    /// Unipolar `0..1` value at phase `p` (in cycles).
    #[inline]
    pub fn at(self, p: f64) -> f32 {
        let p = p.rem_euclid(1.0);
        (match self {
            Wave::Sine => 0.5 + 0.5 * (p * TAU).sin(),
            Wave::Tri => 1.0 - (2.0 * p - 1.0).abs(),
            Wave::Saw => p,
            Wave::Square => {
                if p < 0.5 {
                    1.0
                } else {
                    0.0
                }
            }
        }) as f32
    }
}

/// Interpolation shape of a curve segment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ease {
    Linear,
    /// Smoothstep: slow start, slow finish.
    Smooth,
    /// Quadratic ease-in: slow start, fast finish.
    In,
    /// Quadratic ease-out: fast start, slow finish.
    Out,
}

impl Ease {
    #[inline]
    fn apply(self, x: f32) -> f32 {
        match self {
            Ease::Linear => x,
            Ease::Smooth => x * x * (3.0 - 2.0 * x),
            Ease::In => x * x,
            Ease::Out => 1.0 - (1.0 - x) * (1.0 - x),
        }
    }
}

/// One segment of a [`Curve`]: moves `from → to` over `len` beats starting at
/// song beat `start`, then holds `to`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Seg {
    pub start: f64,
    pub len: f64,
    pub from: f32,
    pub to: f32,
    pub ease: Ease,
}

/// An ADSR shape (seconds; sustain is a level). Used for the amp envelope and
/// for per-voice modulation envelopes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnvShape {
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Unary {
    Neg,
    Abs,
    /// `0..1 → lo..hi`.
    Range(f32, f32),
    /// `0..1 → lo..hi` exponentially (for frequencies).
    RangeX(f32, f32),
    Clamp(f32, f32),
    Pow(f32),
    /// Quantize `0..1` into `n` steps.
    Steps(f32),
    /// Smoothstep on the clamped `0..1` value.
    Ease,
    /// `0..1 → -1..1`.
    Bipolar,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Binary {
    Add,
    Sub,
    Mul,
    Div,
    Max,
    Min,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Expr {
    Const(f32),
    Wave { wave: Wave, period: f64 },
    Curve(Arc<[Seg]>),
    Knob(u8),
    Noise { period: f64, smooth: bool },
    Func(Func),
    Vel,
    Key,
    Rnd,
    Env(EnvShape),
    Lfo { hz: f32, wave: Wave },
    Unary(Unary, Box<Expr>),
    Binary(Binary, Box<Expr>, Box<Expr>),
    /// Evaluate `inner` at `beat * scale + offset`.
    Time { scale: f64, offset: f64, inner: Box<Expr> },
}

impl Expr {
    /// True when the value depends on nothing — it can be folded at compile time.
    fn is_static(&self) -> bool {
        match self {
            Expr::Const(_) => true,
            Expr::Unary(_, a) => a.is_static(),
            Expr::Binary(_, a, b) => a.is_static() && b.is_static(),
            Expr::Time { inner, .. } => inner.is_static(),
            _ => false,
        }
    }

    /// Whether any node reads per-voice state.
    fn is_voice(&self) -> bool {
        match self {
            Expr::Vel | Expr::Key | Expr::Rnd | Expr::Env(_) | Expr::Lfo { .. } => true,
            Expr::Unary(_, a) => a.is_voice(),
            Expr::Binary(_, a, b) => a.is_voice() || b.is_voice(),
            Expr::Time { inner, .. } => inner.is_voice(),
            _ => false,
        }
    }
}

// ── Closure escape hatch ──

/// What a [`Signal::func`] closure sees.
#[derive(Clone, Copy)]
pub struct Ctx {
    /// Song position in beats (moves with `s.jump` / `s.hold`).
    pub beat: f64,
    knobs: [f32; 128],
}

impl Ctx {
    pub fn new(beat: f64, knobs: &[f32; 128]) -> Self {
        Self { beat, knobs: *knobs }
    }

    /// Song position in bars.
    pub fn bar(&self) -> f64 {
        self.beat / 4.0
    }

    /// MIDI CC `cc`, normalized `0..1`.
    pub fn knob(&self, cc: u8) -> f32 {
        self.knobs[(cc & 127) as usize]
    }

    pub fn during(&self, s: Section) -> bool {
        s.contains_beat(self.beat)
    }

    pub fn after(&self, s: Section) -> bool {
        self.beat >= s.start_beat()
    }

    /// `0..1` progress through `s` (clamped).
    pub fn progress(&self, s: Section) -> f32 {
        s.progress(self.beat)
    }
}

/// A shared, stateless closure `Ctx → f32`. Compared by identity, so a closure
/// signal counts as "changed" whenever its builder re-runs.
#[derive(Clone)]
pub struct Func(Arc<dyn Fn(Ctx) -> f32 + Send + Sync>);

impl PartialEq for Func {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl fmt::Debug for Func {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Func(..)")
    }
}

// ── Public signal types ──

/// A song-time value: constant, LFO, curve, knob, or any arithmetic of those.
#[derive(Clone, Debug, PartialEq)]
pub struct Signal(pub(crate) Expr);

/// A per-voice value: anything a [`Signal`] can be, plus envelopes, velocity,
/// key tracking and voice LFOs.
#[derive(Clone, Debug, PartialEq)]
pub struct Mod(pub(crate) Expr);

impl Signal {
    /// The closure escape hatch, for automation no combinator expresses.
    /// Keep it arithmetic — it runs on the audio thread at control rate.
    pub fn func(f: impl Fn(Ctx) -> f32 + Send + Sync + 'static) -> Signal {
        Signal(Expr::Func(Func(Arc::new(f))))
    }

    pub fn constant(v: f32) -> Signal {
        Signal(Expr::Const(v))
    }

    /// The value if this signal is a plain constant.
    pub fn as_const(&self) -> Option<f32> {
        match self.0 {
            Expr::Const(v) => Some(v),
            _ => None,
        }
    }
}

impl Mod {
    pub fn constant(v: f32) -> Mod {
        Mod(Expr::Const(v))
    }

    /// Whether this modulation reads per-voice state (envelope, velocity, …).
    pub fn is_voice(&self) -> bool {
        self.0.is_voice()
    }
}

impl From<f32> for Signal {
    fn from(v: f32) -> Self {
        Signal(Expr::Const(v))
    }
}

impl From<f32> for Mod {
    fn from(v: f32) -> Self {
        Mod(Expr::Const(v))
    }
}

impl From<Signal> for Mod {
    fn from(s: Signal) -> Self {
        Mod(s.0)
    }
}

impl From<Curve> for Signal {
    fn from(c: Curve) -> Self {
        c.signal()
    }
}

impl From<Curve> for Mod {
    fn from(c: Curve) -> Self {
        Mod(c.signal().0)
    }
}

// ── Sources ──

fn wave(wave: Wave, period: f32) -> Signal {
    Signal(Expr::Wave { wave, period: period as f64 })
}

/// Sine LFO, `0..1`, one cycle every `period` beats (starts at 0.5, rising).
pub fn sine(period: f32) -> Signal {
    wave(Wave::Sine, period)
}

/// Triangle LFO, `0..1`, starting at 0.
pub fn tri(period: f32) -> Signal {
    wave(Wave::Tri, period)
}

/// Rising sawtooth, `0..1` — a repeating ramp.
pub fn saw(period: f32) -> Signal {
    wave(Wave::Saw, period)
}

/// Square wave: 1 for the first half of each period, 0 for the second.
pub fn square(period: f32) -> Signal {
    wave(Wave::Square, period)
}

/// MIDI CC `cc`, normalized `0..1` (0 until the knob moves).
pub fn knob(cc: u8) -> Signal {
    Signal(Expr::Knob(cc & 127))
}

/// A new random value every `period` beats (seeded — renders reproduce).
pub fn noise(period: f32) -> Signal {
    Signal(Expr::Noise { period: period as f64, smooth: false })
}

/// Random values every `period` beats, smoothly interpolated.
pub fn smooth_noise(period: f32) -> Signal {
    Signal(Expr::Noise { period: period as f64, smooth: true })
}

/// 0 before `s` starts, 1 from then on.
pub fn after(s: Section) -> Signal {
    let at = s.start_beat();
    Signal(Expr::Curve(Arc::from([Seg { start: at, len: 0.0, from: 0.0, to: 1.0, ease: Ease::Linear }])))
}

/// 1 inside `s`, 0 outside.
pub fn during(s: Section) -> Signal {
    let (a, b) = (s.start_beat(), s.end_beat());
    Signal(Expr::Curve(Arc::from([
        Seg { start: a, len: 0.0, from: 0.0, to: 1.0, ease: Ease::Linear },
        Seg { start: b, len: 0.0, from: 1.0, to: 0.0, ease: Ease::Linear },
    ])))
}

/// A per-voice attack/decay envelope, `0..1`, restarted on every note.
pub fn env(attack: f32, decay: f32) -> Mod {
    adsr(attack, decay, 0.0, decay)
}

/// A per-voice ADSR modulation envelope, `0..1`.
pub fn adsr(attack: f32, decay: f32, sustain: f32, release: f32) -> Mod {
    Mod(Expr::Env(EnvShape { attack, decay, sustain, release }))
}

/// Note velocity, `0..1`.
pub fn vel() -> Mod {
    Mod(Expr::Vel)
}

/// Key tracking: octaves above middle C (`C5` → 1.0, `C3` → -2.0).
pub fn key() -> Mod {
    Mod(Expr::Key)
}

/// A random value per note, `0..1`.
pub fn rnd() -> Mod {
    Mod(Expr::Rnd)
}

/// A per-voice sine LFO in Hz, restarted on every note (vibrato, wobble).
pub fn lfo(hz: f32) -> Mod {
    Mod(Expr::Lfo { hz, wave: Wave::Sine })
}

// ── Combinators (identical surface on Signal and Mod) ──

macro_rules! combinators {
    ($t:ident) => {
        impl $t {
            fn unary(self, op: Unary) -> Self {
                $t(Expr::Unary(op, Box::new(self.0)))
            }

            fn binary(self, op: Binary, other: Self) -> Self {
                $t(Expr::Binary(op, Box::new(self.0), Box::new(other.0)))
            }

            fn time(self, scale: f64, offset: f64) -> Self {
                $t(Expr::Time { scale, offset, inner: Box::new(self.0) })
            }

            /// Map `0..1` onto `lo..hi`.
            pub fn range(self, lo: f32, hi: f32) -> Self {
                self.unary(Unary::Range(lo, hi))
            }

            /// Map `0..1` onto `lo..hi` exponentially — the right mapping for
            /// frequencies (`rangex(200.0, 8000.0)` sweeps evenly by ear).
            pub fn rangex(self, lo: f32, hi: f32) -> Self {
                self.unary(Unary::RangeX(lo.max(1e-6), hi.max(1e-6)))
            }

            pub fn clamp(self, lo: f32, hi: f32) -> Self {
                self.unary(Unary::Clamp(lo, hi))
            }

            pub fn pow(self, k: f32) -> Self {
                self.unary(Unary::Pow(k))
            }

            /// Quantize `0..1` into `n` equal steps.
            pub fn steps(self, n: u32) -> Self {
                self.unary(Unary::Steps(n.max(1) as f32))
            }

            /// Smoothstep: soften the ends of a `0..1` movement.
            pub fn ease(self) -> Self {
                self.unary(Unary::Ease)
            }

            /// `0..1 → -1..1`.
            pub fn bipolar(self) -> Self {
                self.unary(Unary::Bipolar)
            }

            pub fn abs(self) -> Self {
                self.unary(Unary::Abs)
            }

            pub fn max(self, other: impl Into<$t>) -> Self {
                self.binary(Binary::Max, other.into())
            }

            pub fn min(self, other: impl Into<$t>) -> Self {
                self.binary(Binary::Min, other.into())
            }

            /// Run `k` times faster.
            pub fn fast(self, k: f32) -> Self {
                self.time(k as f64, 0.0)
            }

            /// Run `k` times slower.
            pub fn slow(self, k: f32) -> Self {
                self.time(1.0 / k.max(1e-6) as f64, 0.0)
            }

            /// Shift later in time by `beats`.
            pub fn shift(self, beats: f32) -> Self {
                self.time(1.0, -(beats as f64))
            }
        }

        impl Neg for $t {
            type Output = $t;
            fn neg(self) -> $t {
                self.unary(Unary::Neg)
            }
        }
    };
}

combinators!(Signal);
combinators!(Mod);

/// Arithmetic operators. Mixing a `Mod` into anything yields a `Mod`; pure
/// signals and numbers stay `Signal`.
macro_rules! arith {
    ($trait:ident, $method:ident, $op:ident; $($l:ty, $r:ty => $out:ident;)*) => {$(
        impl $trait<$r> for $l {
            type Output = $out;
            fn $method(self, rhs: $r) -> $out {
                let (a, b): ($out, $out) = (self.into(), rhs.into());
                $out(Expr::Binary(Binary::$op, Box::new(a.0), Box::new(b.0)))
            }
        }
    )*};
}

macro_rules! arith_all {
    ($trait:ident, $method:ident, $op:ident) => {
        arith!($trait, $method, $op;
            Signal, Signal => Signal;
            Signal, f32 => Signal;
            f32, Signal => Signal;
            Mod, Mod => Mod;
            Mod, Signal => Mod;
            Signal, Mod => Mod;
            Mod, f32 => Mod;
            f32, Mod => Mod;
        );
    };
}

arith_all!(Add, add, Add);
arith_all!(Sub, sub, Sub);
arith_all!(Mul, mul, Mul);
arith_all!(Div, div, Div);

// ── Curves over the song form ──

/// Build a piecewise song-time curve over [`Section`]s:
///
/// ```ignore
/// curve().hold(INTRO, 0.2).rise(BUILD, 0.2, 1.0).ease().hold(DROP, 1.0).signal()
/// ```
///
/// Before the first segment the curve sits at its first value; between and
/// after segments it holds the last reached value.
#[derive(Clone, Debug, Default)]
pub struct Curve {
    segs: Vec<Seg>,
}

pub fn curve() -> Curve {
    Curve::default()
}

impl Curve {
    fn push(mut self, s: Section, from: f32, to: f32) -> Self {
        self.segs.push(Seg { start: s.start_beat(), len: s.beats(), from, to, ease: Ease::Linear });
        self
    }

    fn set_ease(mut self, ease: Ease) -> Self {
        if let Some(last) = self.segs.last_mut() {
            last.ease = ease;
        }
        self
    }

    /// Hold `v` across `s`.
    pub fn hold(self, s: Section, v: f32) -> Self {
        self.push(s, v, v)
    }

    /// Move `from → to` across `s`.
    pub fn rise(self, s: Section, from: f32, to: f32) -> Self {
        self.push(s, from, to)
    }

    /// Move `from → to` across `s` (reads better for decreasing values).
    pub fn fall(self, s: Section, from: f32, to: f32) -> Self {
        self.push(s, from, to)
    }

    /// Smoothstep the previous segment.
    pub fn ease(self) -> Self {
        self.set_ease(Ease::Smooth)
    }

    /// Ease the previous segment in (slow start).
    pub fn ease_in(self) -> Self {
        self.set_ease(Ease::In)
    }

    /// Ease the previous segment out (slow finish).
    pub fn ease_out(self) -> Self {
        self.set_ease(Ease::Out)
    }

    pub fn signal(mut self) -> Signal {
        self.segs.sort_by(|a, b| a.start.total_cmp(&b.start));
        Signal(Expr::Curve(Arc::from(self.segs)))
    }
}

fn eval_curve(segs: &[Seg], beat: f64) -> f32 {
    let Some(first) = segs.first() else {
        return 0.0;
    };
    if beat < first.start {
        return first.from;
    }
    // The last segment that has started.
    let i = segs.partition_point(|s| s.start <= beat) - 1;
    let s = &segs[i];
    if s.len <= 0.0 || beat >= s.start + s.len {
        return s.to;
    }
    let x = ((beat - s.start) / s.len) as f32;
    s.from + (s.to - s.from) * s.ease.apply(x)
}

fn hash01(i: i64) -> f32 {
    let mut z = (i as u64).wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 40) as f32 / (1u64 << 24) as f32
}

// ── Compilation ──

/// Which per-voice envelopes and LFOs an instrument's programs read, deduplicated
/// into numbered slots. The voice runs exactly these.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VoiceLayout {
    pub envs: Vec<EnvShape>,
    pub lfos: Vec<(f32, Wave)>,
}

impl VoiceLayout {
    fn env_slot(&mut self, e: EnvShape) -> u8 {
        if let Some(i) = self.envs.iter().position(|x| *x == e) {
            return i as u8;
        }
        if self.envs.len() >= MAX_ENVS {
            tracing::warn!("more than {MAX_ENVS} distinct mod envelopes — reusing the last");
            return (MAX_ENVS - 1) as u8;
        }
        self.envs.push(e);
        (self.envs.len() - 1) as u8
    }

    fn lfo_slot(&mut self, l: (f32, Wave)) -> u8 {
        if let Some(i) = self.lfos.iter().position(|x| *x == l) {
            return i as u8;
        }
        if self.lfos.len() >= MAX_LFOS {
            tracing::warn!("more than {MAX_LFOS} distinct voice LFOs — reusing the last");
            return (MAX_LFOS - 1) as u8;
        }
        self.lfos.push(l);
        (self.lfos.len() - 1) as u8
    }
}

#[derive(Clone, Debug)]
enum Op {
    Const(f32),
    Wave(Wave, f64),
    Curve(Arc<[Seg]>),
    Knob(u8),
    Noise(f64, bool),
    Func(Func),
    Vel,
    Key,
    Rnd,
    Env(u8),
    Lfo(u8),
    Unary(Unary),
    Binary(Binary),
    TimePush(f64, f64),
    TimePop,
}

/// Per-voice inputs to a [`Program`]. Zero for global evaluation.
#[derive(Clone, Copy, Debug, Default)]
pub struct VoiceInputs {
    pub vel: f32,
    pub key: f32,
    pub rnd: f32,
    pub env: [f32; MAX_ENVS],
    pub lfo: [f32; MAX_LFOS],
}

/// Everything a [`Program`] may read.
pub struct EvalCtx<'a> {
    /// Song position in beats.
    pub beat: f64,
    pub knobs: &'a [f32; 128],
    pub voice: &'a VoiceInputs,
}

/// A signal compiled to flat postfix ops. Evaluation uses a fixed stack — no
/// allocation, no recursion — so it is safe on the audio thread.
#[derive(Clone, Debug)]
pub struct Program {
    ops: Vec<Op>,
    /// Folded value when the expression depends on nothing.
    konst: Option<f32>,
}

impl Program {
    /// Compile a song-time signal.
    pub fn global(s: &Signal) -> Program {
        Self::compile(&s.0, &mut VoiceLayout::default())
    }

    /// Compile a per-voice modulation, allocating its envelope/LFO slots.
    pub fn voice(m: &Mod, layout: &mut VoiceLayout) -> Program {
        Self::compile(&m.0, layout)
    }

    pub fn constant(v: f32) -> Program {
        Program { ops: vec![Op::Const(v)], konst: Some(v) }
    }

    fn compile(e: &Expr, layout: &mut VoiceLayout) -> Program {
        let mut ops = Vec::new();
        let (mut depth, mut max_depth, mut tdepth, mut max_tdepth) = (0usize, 0usize, 0usize, 0usize);
        emit(e, layout, &mut ops, &mut depth, &mut max_depth, &mut tdepth, &mut max_tdepth);
        if max_depth > STACK || max_tdepth > TIME_STACK {
            tracing::error!("signal expression too deep to evaluate — using 0");
            return Program::constant(0.0);
        }
        let mut p = Program { ops, konst: None };
        if e.is_static() {
            let v = p.eval(&EvalCtx { beat: 0.0, knobs: &[0.0; 128], voice: &VoiceInputs::default() });
            p = Program::constant(v);
        }
        p
    }

    pub fn is_const(&self) -> bool {
        self.konst.is_some()
    }

    pub fn eval(&self, ctx: &EvalCtx) -> f32 {
        if let Some(v) = self.konst {
            return v;
        }
        let mut stack = [0.0f32; STACK];
        let mut sp = 0usize;
        let mut times = [0.0f64; TIME_STACK];
        let mut tp = 0usize;
        let mut beat = ctx.beat;
        macro_rules! push {
            ($v:expr) => {{
                stack[sp] = $v;
                sp += 1;
            }};
        }
        for op in &self.ops {
            match op {
                Op::Const(v) => push!(*v),
                Op::Wave(w, period) => {
                    push!(if *period > 0.0 { w.at(beat / period) } else { 0.0 })
                }
                Op::Curve(segs) => push!(eval_curve(segs, beat)),
                Op::Knob(cc) => push!(ctx.knobs[*cc as usize]),
                Op::Noise(period, smooth) => {
                    let x = if *period > 0.0 { beat / period } else { 0.0 };
                    let i = x.floor();
                    let a = hash01(i as i64);
                    push!(if *smooth {
                        let b = hash01(i as i64 + 1);
                        let t = (x - i) as f32;
                        a + (b - a) * t * t * (3.0 - 2.0 * t)
                    } else {
                        a
                    })
                }
                Op::Func(f) => push!((f.0)(Ctx::new(beat, ctx.knobs))),
                Op::Vel => push!(ctx.voice.vel),
                Op::Key => push!(ctx.voice.key),
                Op::Rnd => push!(ctx.voice.rnd),
                Op::Env(i) => push!(ctx.voice.env[*i as usize]),
                Op::Lfo(i) => push!(ctx.voice.lfo[*i as usize]),
                Op::Unary(u) => {
                    let x = stack[sp - 1];
                    stack[sp - 1] = match *u {
                        Unary::Neg => -x,
                        Unary::Abs => x.abs(),
                        Unary::Range(lo, hi) => lo + (hi - lo) * x,
                        Unary::RangeX(lo, hi) => lo * (hi / lo).powf(x),
                        Unary::Clamp(lo, hi) => x.clamp(lo, hi),
                        Unary::Pow(k) => x.max(0.0).powf(k),
                        Unary::Steps(n) => (x * n).floor().min(n - 1.0).max(0.0) / (n - 1.0).max(1.0),
                        Unary::Ease => {
                            let x = x.clamp(0.0, 1.0);
                            x * x * (3.0 - 2.0 * x)
                        }
                        Unary::Bipolar => x * 2.0 - 1.0,
                    };
                }
                Op::Binary(b) => {
                    sp -= 1;
                    let (x, y) = (stack[sp - 1], stack[sp]);
                    stack[sp - 1] = match b {
                        Binary::Add => x + y,
                        Binary::Sub => x - y,
                        Binary::Mul => x * y,
                        Binary::Div => {
                            if y == 0.0 {
                                0.0
                            } else {
                                x / y
                            }
                        }
                        Binary::Max => x.max(y),
                        Binary::Min => x.min(y),
                    };
                }
                Op::TimePush(scale, offset) => {
                    times[tp] = beat;
                    tp += 1;
                    beat = beat * scale + offset;
                }
                Op::TimePop => {
                    tp -= 1;
                    beat = times[tp];
                }
            }
        }
        if sp == 0 { 0.0 } else { stack[sp - 1] }
    }
}

fn emit(
    e: &Expr,
    layout: &mut VoiceLayout,
    ops: &mut Vec<Op>,
    depth: &mut usize,
    max_depth: &mut usize,
    tdepth: &mut usize,
    max_tdepth: &mut usize,
) {
    let mut leaf = |op: Op, ops: &mut Vec<Op>, depth: &mut usize| {
        ops.push(op);
        *depth += 1;
        *max_depth = (*max_depth).max(*depth);
    };
    match e {
        Expr::Const(v) => leaf(Op::Const(*v), ops, depth),
        Expr::Wave { wave, period } => leaf(Op::Wave(*wave, *period), ops, depth),
        Expr::Curve(segs) => leaf(Op::Curve(segs.clone()), ops, depth),
        Expr::Knob(cc) => leaf(Op::Knob(*cc), ops, depth),
        Expr::Noise { period, smooth } => leaf(Op::Noise(*period, *smooth), ops, depth),
        Expr::Func(f) => leaf(Op::Func(f.clone()), ops, depth),
        Expr::Vel => leaf(Op::Vel, ops, depth),
        Expr::Key => leaf(Op::Key, ops, depth),
        Expr::Rnd => leaf(Op::Rnd, ops, depth),
        Expr::Env(shape) => {
            let slot = layout.env_slot(*shape);
            leaf(Op::Env(slot), ops, depth)
        }
        Expr::Lfo { hz, wave } => {
            let slot = layout.lfo_slot((*hz, *wave));
            leaf(Op::Lfo(slot), ops, depth)
        }
        Expr::Unary(u, a) => {
            emit(a, layout, ops, depth, max_depth, tdepth, max_tdepth);
            ops.push(Op::Unary(*u));
        }
        Expr::Binary(b, x, y) => {
            emit(x, layout, ops, depth, max_depth, tdepth, max_tdepth);
            emit(y, layout, ops, depth, max_depth, tdepth, max_tdepth);
            ops.push(Op::Binary(*b));
            *depth -= 1;
        }
        Expr::Time { scale, offset, inner } => {
            ops.push(Op::TimePush(*scale, *offset));
            *tdepth += 1;
            *max_tdepth = (*max_tdepth).max(*tdepth);
            emit(inner, layout, ops, depth, max_depth, tdepth, max_tdepth);
            *tdepth -= 1;
            ops.push(Op::TimePop);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: impl Into<Signal>, beat: f64) -> f32 {
        let p = Program::global(&s.into());
        p.eval(&EvalCtx { beat, knobs: &[0.0; 128], voice: &VoiceInputs::default() })
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn waves_are_unipolar() {
        assert!(close(at(sine(4.0), 0.0), 0.5));
        assert!(close(at(sine(4.0), 1.0), 1.0));
        assert!(close(at(sine(4.0), 3.0), 0.0));
        assert!(close(at(tri(4.0), 0.0), 0.0));
        assert!(close(at(tri(4.0), 2.0), 1.0));
        assert!(close(at(saw(4.0), 2.0), 0.5));
        assert!(close(at(square(4.0), 1.0), 1.0));
        assert!(close(at(square(4.0), 3.0), 0.0));
    }

    #[test]
    fn range_and_rangex() {
        assert!(close(at(sine(4.0).range(100.0, 300.0), 0.0), 200.0));
        assert!(close(at(saw(4.0).rangex(100.0, 400.0), 2.0), 200.0));
    }

    #[test]
    fn arithmetic_with_numbers_on_both_sides() {
        let s = 100.0 + saw(4.0) * 200.0 - 50.0;
        assert!(close(at(s, 2.0), 150.0));
        let s = 1.0 - saw(4.0);
        assert!(close(at(s, 1.0), 0.75));
        assert!(close(at(-saw(4.0), 2.0), -0.5));
    }

    #[test]
    fn constants_fold() {
        let p = Program::global(&(Signal::from(2.0) * 3.0 + 1.0));
        assert!(p.is_const());
        assert!(close(p.eval(&EvalCtx { beat: 9.0, knobs: &[0.0; 128], voice: &VoiceInputs::default() }), 7.0));
    }

    #[test]
    fn time_transforms() {
        assert!(close(at(saw(4.0).fast(2.0), 1.0), 0.5));
        assert!(close(at(saw(4.0).slow(2.0), 4.0), 0.5));
        assert!(close(at(saw(4.0).shift(1.0), 1.0), 0.0));
        // Time transforms nest and restore.
        let s = saw(4.0).fast(2.0) + saw(4.0);
        assert!(close(at(s, 1.0), 0.75));
    }

    #[test]
    fn curve_holds_rises_and_eases() {
        let a = Section::start(4); // bars 0..4 = beats 0..16
        let b = a.then(4); // beats 16..32
        let c = curve().hold(a, 0.2).rise(b, 0.2, 1.0).signal();
        assert!(close(at(c.clone(), 0.0), 0.2));
        assert!(close(at(c.clone(), 15.9), 0.2));
        assert!(close(at(c.clone(), 24.0), 0.6));
        assert!(close(at(c.clone(), 100.0), 1.0), "holds the last value");
        let eased = curve().rise(b, 0.0, 1.0).ease().signal();
        assert!(close(at(eased.clone(), 0.0), 0.0), "before the first segment");
        assert!(at(eased.clone(), 18.0) < 0.125, "smoothstep starts slow");
        assert!(close(at(eased, 24.0), 0.5));
    }

    #[test]
    fn after_and_during() {
        let s = Section::at(2, 2); // beats 8..16
        assert!(close(at(after(s), 7.9), 0.0));
        assert!(close(at(after(s), 8.0), 1.0));
        assert!(close(at(during(s), 7.9), 0.0));
        assert!(close(at(during(s), 12.0), 1.0));
        assert!(close(at(during(s), 16.0), 0.0));
    }

    #[test]
    fn knob_and_func() {
        let mut knobs = [0.0; 128];
        knobs[7] = 0.25;
        let p = Program::global(&knob(7).range(0.0, 100.0));
        assert!(close(p.eval(&EvalCtx { beat: 0.0, knobs: &knobs, voice: &VoiceInputs::default() }), 25.0));
        let f = Signal::func(|c: Ctx| c.beat as f32 * 2.0 + c.knob(7));
        let p = Program::global(&f);
        assert!(close(p.eval(&EvalCtx { beat: 3.0, knobs: &knobs, voice: &VoiceInputs::default() }), 6.25));
    }

    #[test]
    fn voice_sources_allocate_deduplicated_slots() {
        let mut layout = VoiceLayout::default();
        let m = 300.0 + env(0.0, 0.2) * vel() * 3000.0 + env(0.0, 0.2) * 10.0 + lfo(5.0) * 2.0;
        let p = Program::voice(&m, &mut layout);
        assert_eq!(layout.envs.len(), 1, "identical envelopes share a slot");
        assert_eq!(layout.lfos.len(), 1);
        let mut v = VoiceInputs { vel: 0.5, ..Default::default() };
        v.env[0] = 1.0;
        v.lfo[0] = 0.5;
        let out = p.eval(&EvalCtx { beat: 0.0, knobs: &[0.0; 128], voice: &v });
        assert!(close(out, 300.0 + 1500.0 + 10.0 + 1.0));
        assert!(m.is_voice());
    }

    #[test]
    fn signals_diff_by_value() {
        assert_eq!(sine(4.0).range(1.0, 2.0), sine(4.0).range(1.0, 2.0));
        assert_ne!(sine(4.0).range(1.0, 2.0), sine(4.0).range(1.0, 3.0));
        let f = |c: Ctx| c.beat as f32;
        assert_ne!(Signal::func(f), Signal::func(f), "closures compare by identity");
    }

    #[test]
    fn noise_is_deterministic_and_bounded() {
        for i in 0..50 {
            let v = at(noise(1.0), i as f64 * 0.37);
            assert!((0.0..1.0).contains(&v));
            assert_eq!(v, at(noise(1.0), i as f64 * 0.37));
        }
        let s = at(smooth_noise(4.0), 1.0);
        assert!((0.0..=1.0).contains(&s));
    }
}
