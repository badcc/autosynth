//! Effect chains as data. A chain is an ordered list of nodes: effects (whose
//! parameters are [`Signal`]s, one per slot), parallel branches, and sends to
//! buses. Structural choices (an effect's kind and mode) live in [`FxKind`];
//! changing one rebuilds that node, while parameter edits keep its state —
//! a delay tail survives any knob change.

use std::any::Any;
use std::fmt;
use std::sync::Arc;

use crate::dsp::effects::chorus::{self, Chorus};
use crate::dsp::effects::compressor::{self, Compressor};
use crate::dsp::effects::crush::{self, Crush};
use crate::dsp::effects::delay::{self, Delay, DelayMode};
use crate::dsp::effects::distortion::{self, Distortion, DistortionMode};
use crate::dsp::effects::eq::{self, Eq};
use crate::dsp::effects::filter::{self, FilterFx};
use crate::dsp::effects::gate::{self, Gate};
use crate::dsp::effects::limiter::{self, Limiter};
use crate::dsp::effects::phaser::{self, Phaser};
use crate::dsp::effects::reverb::{self, Reverb};
use crate::dsp::effects::width::{self, Width};
use crate::dsp::effects::{Effect, ParamDef};
use crate::dsp::filter::FilterType;
use crate::model::track::BusId;
use crate::music::signal::Signal;

/// A user-defined effect. Implement this plus `PartialEq` (so hot reload can
/// tell whether it changed) and add it with `c.custom(MyFx { .. })`.
pub trait CustomEffect: fmt::Debug + Send + Sync + 'static {
    /// Automatable parameters, set by slot index.
    fn params(&self) -> &'static [ParamDef] {
        &[]
    }

    /// Construct the DSP (called on the control thread).
    fn build(&self, sample_rate: f32) -> Box<dyn Effect>;
}

trait DynCustom: CustomEffect {
    fn as_any(&self) -> &dyn Any;
    fn dyn_eq(&self, other: &dyn DynCustom) -> bool;
}

impl<T: CustomEffect + PartialEq> DynCustom for T {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn dyn_eq(&self, other: &dyn DynCustom) -> bool {
        other.as_any().downcast_ref::<T>() == Some(self)
    }
}

/// A shared, comparable custom effect.
#[derive(Clone)]
pub struct CustomFx(Arc<dyn DynCustom>);

impl CustomFx {
    pub fn new<T: CustomEffect + PartialEq>(fx: T) -> Self {
        CustomFx(Arc::new(fx))
    }
}

impl PartialEq for CustomFx {
    fn eq(&self, other: &Self) -> bool {
        self.0.dyn_eq(other.0.as_ref())
    }
}

impl fmt::Debug for CustomFx {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Which effect a node is, plus its structural (non-automatable) settings.
#[derive(Clone, Debug, PartialEq)]
pub enum FxKind {
    Delay(DelayMode),
    Chorus,
    Drive(DistortionMode),
    Reverb,
    Filter(FilterType),
    Eq,
    Comp,
    Gate(String),
    Phaser,
    Crush,
    Width,
    Limit,
    Custom(CustomFx),
}

impl FxKind {
    pub fn params(&self) -> &'static [ParamDef] {
        match self {
            FxKind::Delay(_) => delay::PARAMS,
            FxKind::Chorus => chorus::PARAMS,
            FxKind::Drive(_) => distortion::PARAMS,
            FxKind::Reverb => reverb::PARAMS,
            FxKind::Filter(_) => filter::PARAMS,
            FxKind::Eq => eq::PARAMS,
            FxKind::Comp => compressor::PARAMS,
            FxKind::Gate(_) => gate::PARAMS,
            FxKind::Phaser => phaser::PARAMS,
            FxKind::Crush => crush::PARAMS,
            FxKind::Width => width::PARAMS,
            FxKind::Limit => limiter::PARAMS,
            FxKind::Custom(c) => c.0.params(),
        }
    }

    pub fn build(&self, sample_rate: f32) -> Box<dyn Effect> {
        match self {
            FxKind::Delay(mode) => Box::new(Delay::new(sample_rate, *mode)),
            FxKind::Chorus => Box::new(Chorus::new(sample_rate)),
            FxKind::Drive(mode) => Box::new(Distortion::new(*mode)),
            FxKind::Reverb => Box::new(Reverb::new(sample_rate)),
            FxKind::Filter(kind) => Box::new(FilterFx::new(sample_rate, *kind)),
            FxKind::Eq => Box::new(Eq::new()),
            FxKind::Comp => Box::new(Compressor::new()),
            FxKind::Gate(pattern) => Box::new(Gate::new(pattern)),
            FxKind::Phaser => Box::new(Phaser::new()),
            FxKind::Crush => Box::new(Crush::new()),
            FxKind::Width => Box::new(Width::new()),
            FxKind::Limit => Box::new(Limiter::new()),
            FxKind::Custom(c) => c.0.build(sample_rate),
        }
    }

    /// A node of this kind with every parameter at its default.
    pub fn node(self) -> FxNode {
        let params = self.params().iter().map(|p| Signal::constant(p.default)).collect();
        FxNode::Fx { kind: self, params, enabled: true }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum FxNode {
    Fx { kind: FxKind, params: Vec<Signal>, enabled: bool },
    /// Branches each process the input; their outputs are summed.
    Parallel(Vec<ChainSpec>),
    /// Add `amount ×` the signal at this point into a bus; the chain continues.
    Send { bus: BusId, amount: Signal },
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct ChainSpec(pub Vec<FxNode>);
