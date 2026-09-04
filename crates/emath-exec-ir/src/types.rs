//! Core EMIR value and configuration types.

use super::*;

/// Evaluation resource budget. Resource exhaustion is a typed refusal
/// (`EvalFault::BudgetExhausted`) — never partial authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EvalBudget {
    /// Maximum interpreted op steps.
    pub max_steps: u32,
    /// Maximum capability applications.
    pub max_capability_applications: u32,
}

impl Default for EvalBudget {
    fn default() -> Self {
        Self {
            max_steps: u32::MAX,
            max_capability_applications: 1024,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EmirValue(pub u32);

/// One axis of [`EmirOp::TensorSlice`]: a scalar point or a half-open range.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EmirSliceAxis {
    Point(EmirValue),
    Range { start: EmirValue, end: EmirValue },
}

/// Accumulation strategy for [`EmirOp::Fold`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FoldCombine {
    Add,
    Mul,
    And,
    Or,
}

/// How out-of-range stencil indices resolve: `Clamp` (replicate the edge
/// cell), `Neumann` (mirror the next interior cell), `OneSided` (linear
/// extrapolation; first-order one-sided first differences), or `Dirichlet`
/// (fixed boundary values). 2D and 3D admit
/// `Clamp`/`Neumann`/`OneSided`; fixed Dirichlet faces remain unsupported.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EdgePolicy {
    Clamp,
    Neumann,
    OneSided,
    Dirichlet { left: f64, right: f64 },
}

/// The admitted distribution families of the probability nucleus
/// The `u8` code is the stable kernel encoding (codegen
/// renders it; the rt wrappers decode it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProbKind {
    /// Normal(μ, σ) — Box–Muller sampling, exact density.
    Normal,
    /// Uniform(a, b) — affine map of [0, 1), exact density.
    Uniform,
    /// Bernoulli(p) — threshold sampling (p ∈ {0, 1} exact), PMF.
    Bernoulli,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VectorScalarOp {
    Add,
    Sub,
    Mul,
    Div,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReduceId {
    Sum,
    Max,
    Min,
}

impl ReduceId {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sum => "sum",
            Self::Max => "max",
            Self::Min => "min",
        }
    }
}

impl ProbKind {
    /// The rt kernel's `u8` encoding.
    #[must_use]
    pub fn code(self) -> u8 {
        match self {
            Self::Normal => 0,
            Self::Uniform => 1,
            Self::Bernoulli => 2,
        }
    }

    /// SSA/cell-surface name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Uniform => "uniform",
            Self::Bernoulli => "bernoulli",
        }
    }
}
