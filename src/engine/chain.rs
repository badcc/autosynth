//! The running effect chain. Mirrors a [`ChainSpec`]: DSP nodes are built on
//! the control thread, and edits arrive as per-node [`NodeUpdate`]s so a node
//! whose kind didn't change keeps its state (delay and reverb tails survive
//! parameter edits and enable toggles).

use crate::dsp::StereoFrame;
use crate::dsp::effects::{Effect, FxCtx};
use crate::engine::CONTROL_BLOCK;
use crate::model::chain::{ChainSpec, FxNode};
use crate::music::signal::{EvalCtx, Program, VoiceInputs};

/// Somewhere a `Send` node can deliver audio: the engine's buses.
pub trait SendTarget {
    fn send(&mut self, bus: &str, buf: &[StereoFrame], gain: f32);
}

pub enum RunNode {
    Fx { fx: Box<dyn Effect>, params: Vec<Program>, last: Vec<f32>, enabled: bool },
    Parallel { branches: Vec<FxChain>, scratch: Vec<Vec<StereoFrame>> },
    Send { bus: String, amount: Program },
}

/// How to get from the engine's current node to the new one.
pub enum NodeUpdate {
    /// Same effect kind: keep the DSP state, swap parameters and enable flag.
    Keep { params: Vec<Program>, enabled: bool },
    /// Same branch count: update each branch in place.
    Parallel(Vec<Vec<NodeUpdate>>),
    /// Anything else: a freshly built node.
    Build(RunNode),
}

#[derive(Default)]
pub struct FxChain {
    nodes: Vec<RunNode>,
}

fn build_node(node: &FxNode, sample_rate: f32) -> RunNode {
    match node {
        FxNode::Fx { kind, params, enabled } => {
            let params: Vec<Program> = params.iter().map(Program::global).collect();
            let last = vec![f32::NAN; params.len()];
            RunNode::Fx { fx: kind.build(sample_rate), params, last, enabled: *enabled }
        }
        FxNode::Parallel(branches) => RunNode::Parallel {
            branches: branches.iter().map(|b| FxChain::build(b, sample_rate)).collect(),
            scratch: branches.iter().map(|_| vec![[0.0; 2]; CONTROL_BLOCK]).collect(),
        },
        FxNode::Send { bus, amount } => RunNode::Send { bus: bus.key.clone(), amount: Program::global(amount) },
    }
}

impl FxChain {
    /// Build a chain from scratch (control thread).
    pub fn build(spec: &ChainSpec, sample_rate: f32) -> FxChain {
        FxChain { nodes: spec.0.iter().map(|n| build_node(n, sample_rate)).collect() }
    }

    /// The updates that turn a chain built from `prev` into one for `new`
    /// (control thread).
    pub fn diff(prev: &ChainSpec, new: &ChainSpec, sample_rate: f32) -> Vec<NodeUpdate> {
        new.0
            .iter()
            .enumerate()
            .map(|(i, node)| match (prev.0.get(i), node) {
                (Some(FxNode::Fx { kind: a, .. }), FxNode::Fx { kind: b, params, enabled }) if a == b => {
                    NodeUpdate::Keep { params: params.iter().map(Program::global).collect(), enabled: *enabled }
                }
                (Some(FxNode::Parallel(a)), FxNode::Parallel(b)) if a.len() == b.len() => {
                    NodeUpdate::Parallel(a.iter().zip(b).map(|(x, y)| FxChain::diff(x, y, sample_rate)).collect())
                }
                _ => NodeUpdate::Build(build_node(node, sample_rate)),
            })
            .collect()
    }

    /// Apply updates from [`FxChain::diff`] (audio thread).
    pub fn apply(&mut self, updates: Vec<NodeUpdate>) {
        let mut old: Vec<Option<RunNode>> = std::mem::take(&mut self.nodes).into_iter().map(Some).collect();
        for (i, update) in updates.into_iter().enumerate() {
            let prev = old.get_mut(i).and_then(Option::take);
            let node = match (update, prev) {
                (NodeUpdate::Build(node), _) => node,
                (NodeUpdate::Keep { params, enabled }, Some(RunNode::Fx { fx, .. })) => {
                    let last = vec![f32::NAN; params.len()];
                    RunNode::Fx { fx, params, last, enabled }
                }
                (NodeUpdate::Parallel(branches), Some(RunNode::Parallel { branches: mut old_branches, scratch })) => {
                    for (b, u) in old_branches.iter_mut().zip(branches) {
                        b.apply(u);
                    }
                    RunNode::Parallel { branches: old_branches, scratch }
                }
                _ => {
                    tracing::error!("fx chain out of sync with the scene — dropping a node");
                    continue;
                }
            };
            self.nodes.push(node);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn process<S: SendTarget + ?Sized>(&mut self, buf: &mut [StereoFrame], ctx: &FxCtx, knobs: &[f32; 128], sends: &mut S) {
        let voice = VoiceInputs::default();
        let eval = EvalCtx { beat: ctx.beat, knobs, voice: &voice };
        for node in &mut self.nodes {
            match node {
                RunNode::Fx { fx, params, last, enabled } => {
                    for (slot, (p, l)) in params.iter().zip(last.iter_mut()).enumerate() {
                        let v = p.eval(&eval);
                        if v != *l {
                            fx.set(slot, v);
                            *l = v;
                        }
                    }
                    if *enabled {
                        fx.process(buf, ctx);
                    }
                }
                RunNode::Parallel { branches, scratch } => {
                    let n = buf.len();
                    for (b, s) in branches.iter_mut().zip(scratch.iter_mut()) {
                        s[..n].copy_from_slice(buf);
                        b.process(&mut s[..n], ctx, knobs, sends);
                    }
                    for (i, frame) in buf.iter_mut().enumerate() {
                        *frame = [0.0; 2];
                        for s in scratch.iter() {
                            frame[0] += s[i][0];
                            frame[1] += s[i][1];
                        }
                    }
                }
                RunNode::Send { bus, amount } => sends.send(bus, buf, amount.eval(&eval)),
            }
        }
    }
}
