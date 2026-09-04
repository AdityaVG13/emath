//! Semantic VM (schema `emath.vm`, version 1): an explicit-stack executor
//! for first-order terms with step metering, a deterministic trace and
//! continuation-ready state.
//!
//! Owned frame/value stacks; computes exactly what [`crate::evaluate`]
//! computes. One step per processed frame; budget exhaustion suspends
//! into a [`VmContinuation`] that resumes losslessly. Errors are typed,
//! never stringly encoded.

use crate::{Environment, EvalError, FirstOrderWorld};
use emath_term::{SymbolId, Term, VariableId};
use emath_world_ir::fnv1a64;

/// VM schema id.
pub const VM_SCHEMA: &str = "emath.vm";
/// VM schema version.
pub const VM_SCHEMA_VERSION: u32 = 1;

/// Per-run step budget; total steps accumulate across resumes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VmBudget {
    /// Maximum frames processed in one run.
    pub max_steps: u64,
}

impl VmBudget {
    /// Genesis seed-lane default: generous for seed terms, a hard ceiling.
    #[must_use]
    pub const fn seed_default() -> Self {
        Self { max_steps: 4096 }
    }
}

/// One deterministic trace entry (one processed frame).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VmStep {
    /// A free variable was resolved from the environment.
    Variable(VariableId),
    /// A nullary symbol was resolved by the world.
    Constant(SymbolId),
    /// An application was entered (arguments scheduled).
    Enter {
        /// Operator symbol.
        operator: SymbolId,
        /// Argument count.
        arity: usize,
    },
    /// An operator was applied to its evaluated arguments.
    Apply {
        /// Operator symbol.
        operator: SymbolId,
        /// Argument count.
        arity: usize,
    },
}

impl VmStep {
    /// Canonical single-line encoding.
    #[must_use]
    pub fn canonical(&self) -> String {
        match self {
            Self::Variable(variable) => format!("var {}", variable.0),
            Self::Constant(symbol) => format!("const {}", symbol.0),
            Self::Enter { operator, arity } => format!("enter {}/{arity}", operator.0),
            Self::Apply { operator, arity } => format!("apply {}/{arity}", operator.0),
        }
    }
}

/// Deterministic execution trace.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VmTrace {
    /// Steps in execution order.
    pub steps: Vec<VmStep>,
}

impl VmTrace {
    /// Canonical multi-line encoding (schema header + one row per step).
    #[must_use]
    pub fn canonical(&self) -> String {
        let mut out = format!("{VM_SCHEMA}.v{VM_SCHEMA_VERSION}\n");
        for step in &self.steps {
            out.push_str(&step.canonical());
            out.push('\n');
        }
        out
    }

    /// Deterministic trace identity.
    #[must_use]
    pub fn identity(&self) -> u64 {
        fnv1a64(self.canonical().as_bytes())
    }
}

/// A work frame: either a term to evaluate or a pending application.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Frame {
    Eval(Term),
    Apply { operator: SymbolId, arity: usize },
}

/// Owned machine state, continuation-ready by construction.
#[derive(Clone, Debug, PartialEq)]
pub struct VmState<V> {
    frames: Vec<Frame>,
    values: Vec<V>,
    /// Total steps across every run and resume.
    pub steps: u64,
    /// Deterministic trace across every run and resume.
    pub trace: VmTrace,
}

/// A suspended machine, resumable with a fresh budget.
#[derive(Clone, Debug, PartialEq)]
pub struct VmContinuation<V> {
    state: VmState<V>,
}

impl<V> VmContinuation<V> {
    /// Steps consumed so far.
    #[must_use]
    pub fn steps(&self) -> u64 {
        self.state.steps
    }
}

/// Outcome of a metered run: completion or explicit suspension. Failure is
/// a typed error on the surrounding `Result`, never a silent state.
#[derive(Clone, Debug, PartialEq)]
pub enum VmOutcome<V> {
    /// The term evaluated to a value.
    Complete {
        /// Result value.
        value: V,
        /// Total steps consumed.
        steps: u64,
        /// Deterministic trace.
        trace: VmTrace,
    },
    /// Budget exhausted mid-term; resume with a fresh budget.
    Suspended(VmContinuation<V>),
}

/// Runs `term` in `world` under `environment` with a per-run budget.
pub fn run<W: FirstOrderWorld>(
    term: &Term,
    world: &W,
    environment: &Environment<W::Value>,
    budget: &VmBudget,
) -> Result<VmOutcome<W::Value>, W::Error>
where
    W::Error: From<EvalError>,
{
    let state = VmState {
        frames: vec![Frame::Eval(term.clone())],
        values: Vec::new(),
        steps: 0,
        trace: VmTrace::default(),
    };
    drive(state, world, environment, *budget)
}

/// Resumes a suspended machine with a fresh budget.
pub fn resume<W: FirstOrderWorld>(
    continuation: VmContinuation<W::Value>,
    world: &W,
    environment: &Environment<W::Value>,
    budget: &VmBudget,
) -> Result<VmOutcome<W::Value>, W::Error>
where
    W::Error: From<EvalError>,
{
    drive(continuation.state, world, environment, *budget)
}

fn drive<W: FirstOrderWorld>(
    mut state: VmState<W::Value>,
    world: &W,
    environment: &Environment<W::Value>,
    budget: VmBudget,
) -> Result<VmOutcome<W::Value>, W::Error>
where
    W::Error: From<EvalError>,
{
    let mut run_steps: u64 = 0;
    while let Some(frame) = state.frames.pop() {
        if run_steps >= budget.max_steps {
            // Push the unprocessed frame back so nothing is lost.
            state.frames.push(frame);
            return Ok(VmOutcome::Suspended(VmContinuation { state }));
        }
        run_steps += 1;
        state.steps += 1;
        match frame {
            Frame::Eval(Term::Variable(variable)) => {
                state.trace.steps.push(VmStep::Variable(variable.clone()));
                let value = environment
                    .get(&variable)
                    .cloned()
                    .ok_or_else(|| EvalError::MissingVariable(variable.clone()))?;
                state.values.push(value);
            }
            Frame::Eval(Term::Constant(symbol)) => {
                state.trace.steps.push(VmStep::Constant(symbol.clone()));
                state.values.push(world.constant(&symbol)?);
            }
            Frame::Eval(Term::Apply {
                operator,
                arguments,
            }) => {
                state.trace.steps.push(VmStep::Enter {
                    operator: operator.clone(),
                    arity: arguments.len(),
                });
                state.frames.push(Frame::Apply {
                    operator,
                    arity: arguments.len(),
                });
                // Reverse push so arguments evaluate left to right.
                for argument in arguments.into_iter().rev() {
                    state.frames.push(Frame::Eval(argument));
                }
            }
            Frame::Apply { operator, arity } => {
                state.trace.steps.push(VmStep::Apply {
                    operator: operator.clone(),
                    arity,
                });
                let split =
                    state
                        .values
                        .len()
                        .checked_sub(arity)
                        .ok_or_else(|| EvalError::Arity {
                            symbol: operator.clone(),
                            expected: arity,
                            actual: state.values.len(),
                        })?;
                let arguments = state.values.split_off(split);
                state.values.push(world.apply(&operator, arguments)?);
            }
        }
    }
    let value = state.values.pop().ok_or_else(|| EvalError::Arity {
        symbol: SymbolId("<empty>".into()),
        expected: 1,
        actual: 0,
    })?;
    Ok(VmOutcome::Complete {
        value,
        steps: state.steps,
        trace: state.trace,
    })
}
