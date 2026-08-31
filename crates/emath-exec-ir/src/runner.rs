//! Declaration runner: constructor requires → Self state → definitions →
//! example `given`/`expect` verdicts. Binding rules copy the generated
//! `#[test]`: givens lower in `BTreeMap` order; constructor params and
//! declaration inputs must appear in `given`; definitions lower against
//! inputs, prior definitions (source order, let-binding semantics) and
//! `state.<name>`; `expect` lowers against givens plus definitions
//! (`expect: None` is a worked `Computed` run, no pass/fail claim); a
//! zero-example declaration still gets a `_pane` run when all inputs are
//! bound (`extra_given` adds it to any source examples).

use crate::interp::{EvalFault, Value};
use emath_ir::{Declaration, ExprId, SemanticPackage};
use std::collections::BTreeMap;
use std::fmt;

mod eval;
mod run;
mod simulate;

pub use eval::eval_definitions_values;
pub use run::{run_declaration, run_declaration_with_given, run_package, run_package_with_given};
pub use simulate::{
    Continuation, DAEDisposition, DAEIndex, InitializationVerdict, SimulateOptions, StepMethod,
    Trajectory, TrajectorySample, simulate_continuous, simulate_continuous_dispositioned,
    simulate_continuous_with, step_continuous, step_continuous_values,
};

/// Hint stored on declarations that have no `tests:` examples and cannot
/// be computed directly (an input or constructor parameter is unbound).
pub const ZERO_TEST_NOTE: &str = "no examples; add a worked example or use input fields";

/// Synthetic worked-run name used when the pane supplies givens or when a
/// declaration has no examples and every input is already bound.
pub const PANE_TEST_NAME: &str = "_pane";

/// Outcome of one example test.
#[derive(Clone, Debug, PartialEq)]
pub enum TestVerdict {
    /// `expect` evaluated to `true`.
    Passed,
    /// `expect` evaluated to `false`.
    Failed,
    /// No `expect`: values were computed, no assertion claim.
    Computed,
    /// A constructor `require` / `ensure` evaluated to `false`.
    ConstructorRefused {
        /// Source-like obligation text (`require scale >= 0`).
        obligation: String,
    },
    /// EMIR lowering refused the given, require, assignment, definition, or
    /// expect expression.
    LoweringRefused {
        /// Lowering error text.
        detail: String,
    },
    /// Interpreter fault (type confusion, missing slot, bad register).
    Fault {
        /// The typed fault.
        fault: EvalFault,
    },
}

impl TestVerdict {
    /// Whether this verdict is a passing expect.
    #[must_use]
    pub const fn expect_passed(&self) -> bool {
        matches!(self, Self::Passed)
    }

    /// Whether this verdict is a typed refusal rather than a Boolean fail.
    #[must_use]
    pub const fn is_refused(&self) -> bool {
        matches!(
            self,
            Self::ConstructorRefused { .. } | Self::LoweringRefused { .. } | Self::Fault { .. }
        )
    }

    /// Stable refusal tag for JSON (`constructor-refused` / …), if any.
    #[must_use]
    pub const fn refusal_tag(&self) -> Option<&'static str> {
        match self {
            Self::ConstructorRefused { .. } => Some("constructor-refused"),
            Self::LoweringRefused { .. } => Some("lowering-refused"),
            Self::Fault { .. } => Some("fault"),
            Self::Passed | Self::Failed | Self::Computed => None,
        }
    }

    /// Whether this is a worked example (no `expect`).
    #[must_use]
    pub const fn is_computed(&self) -> bool {
        matches!(self, Self::Computed)
    }

    /// Human-readable refusal / fault text.
    #[must_use]
    pub fn reason_text(&self) -> Option<String> {
        match self {
            Self::ConstructorRefused { obligation } => Some(obligation.clone()),
            Self::LoweringRefused { detail } => Some(detail.clone()),
            Self::Fault { fault } => Some(fault.to_string()),
            Self::Passed | Self::Failed | Self::Computed => None,
        }
    }
}

impl fmt::Display for TestVerdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Passed => f.write_str("passed"),
            Self::Failed => f.write_str("failed"),
            Self::Computed => f.write_str("computed"),
            Self::ConstructorRefused { obligation } => {
                write!(f, "constructor refused: {obligation}")
            }
            Self::LoweringRefused { detail } => write!(f, "lowering refused: {detail}"),
            Self::Fault { fault } => write!(f, "fault: {fault}"),
        }
    }
}

/// One example test after interpretation.
#[derive(Clone, Debug, PartialEq)]
pub struct TestRun {
    /// Example name (`three_squared`).
    pub name: String,
    /// Evaluated `given` map (name → typed [`Value`]), `BTreeMap` order.
    pub given: BTreeMap<String, Value>,
    /// Constructor `Self:` fields when construction succeeded.
    pub state: BTreeMap<String, Value>,
    /// Each definition's computed value, declaration-map order.
    pub definitions: BTreeMap<String, Value>,
    /// Declared outputs that have a computed definition.
    pub outputs: BTreeMap<String, Value>,
    /// Pass / fail / typed refusal.
    pub verdict: TestVerdict,
}

/// Aggregate counts over every example that was attempted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RunSummary {
    /// Tests attempted (excludes zero-test declarations).
    pub tests: u32,
    /// `expect` was true.
    pub passed: u32,
    /// `expect` was false.
    pub failed: u32,
    /// Constructor / lowering / fault refusal.
    pub refused: u32,
    /// Worked examples (`expect` omitted).
    pub computed: u32,
}

/// Per-declaration run.
#[derive(Clone, Debug, PartialEq)]
pub struct DeclarationRun {
    /// Declaration leaf name.
    pub name: String,
    /// Example results in declaration test-id order.
    pub tests: Vec<TestRun>,
    /// Executable law metadata, copied from SIR without reinterpretation.
    pub law_metadata: Option<emath_ir::LawMetadata>,
    /// Present when `tests` is empty (the wasm layer surfaces this as a hint).
    pub note: Option<String>,
}

/// Package-wide report. Declaration order matches [`SemanticPackage`].
#[derive(Clone, Debug, PartialEq)]
pub struct RunReport {
    /// One entry per declaration, source order.
    pub declarations: Vec<DeclarationRun>,
    /// Counts over every attempted example.
    pub summary: RunSummary,
}

/// Definitions are let-bindings admitted in source order, so evaluation
/// follows the same order; the expression spans recover it (programmatic
/// IR with default spans keeps the stable name-keyed order).
pub fn definition_order<'d>(
    package: &SemanticPackage,
    declaration: &'d Declaration,
) -> Vec<(&'d String, ExprId)> {
    let mut entries: Vec<(&'d String, ExprId)> = declaration
        .definitions
        .iter()
        .map(|(name, expr)| (name, *expr))
        .collect();
    entries.sort_by_key(|(_, expr)| package.expr_span(*expr).start);
    entries
}
