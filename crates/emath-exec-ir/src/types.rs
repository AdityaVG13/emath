//! Core EMIR value and configuration types.

/// Evaluation resource budget. Resource exhaustion is a typed refusal
/// (`EvalFault::BudgetExhausted`), never partial authority.
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
