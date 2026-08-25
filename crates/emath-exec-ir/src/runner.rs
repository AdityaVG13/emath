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
pub use run::{
    run_declaration, run_declaration_with_given, run_package, run_package_with_given,
};
pub use simulate::{
    step_continuous, step_continuous_values, simulate_continuous, simulate_continuous_with,
    SimulateOptions, StepMethod, Trajectory, TrajectorySample,
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

#[cfg(test)]
mod tests {
    use super::*;
    use emath_core::{QualifiedName, Span};
    use emath_ir::{
        BinaryOp, Constructor, DeclarationId, ExprNode, Field, Literal, TypeNode, Visibility,
    };

    fn float_field(name: &str, ty: emath_ir::TypeId) -> Field {
        Field {
            name: name.to_string(),
            ty,
            visibility: Visibility::Public,
            source: Span::default(),
        }
    }

    fn square_package(expect_rhs: &str) -> SemanticPackage {
        let mut package = SemanticPackage::new();
        let ty = package.push_type(TypeNode::Float64);
        let x = package.push_expr(
            ExprNode::Variable(QualifiedName::single("x")),
            Span::default(),
        );
        let y_def = package.push_expr(
            ExprNode::Binary {
                operation: BinaryOp::StrictFloatMul,
                left: x,
                right: x,
            },
            Span::default(),
        );
        let three = package.push_expr(
            ExprNode::Literal(Literal::Integer("3".to_string())),
            Span::default(),
        );
        let y = package.push_expr(
            ExprNode::Variable(QualifiedName::single("y")),
            Span::default(),
        );
        let nine = package.push_expr(
            ExprNode::Literal(Literal::Integer(expect_rhs.to_string())),
            Span::default(),
        );
        let expect = package.push_expr(
            ExprNode::Binary {
                operation: BinaryOp::Equal,
                left: y,
                right: nine,
            },
            Span::default(),
        );
        let mut given = BTreeMap::new();
        given.insert("x".to_string(), three);
        let test_id = package.push_test(emath_ir::TestCase {
            name: "three_squared".to_string(),
            given,
            expect: Some(expect),
            source: Span::default(),
        });
        let mut definitions = BTreeMap::new();
        definitions.insert("y".to_string(), y_def);
        package.declarations.push(emath_ir::Declaration {
            id: DeclarationId(0),
            name: QualifiedName::single("Square"),
            kind: QualifiedName::single("function"),
            kind_label: "function".to_string(),
            inputs: vec![float_field("x", ty)],
            outputs: vec![float_field("y", ty)],
            state: Vec::new(),
            algebraic: Vec::new(),
            constructors: Vec::new(),
            definitions,
            invariants: Vec::new(),
            goals: Vec::new(),
            tests: vec![test_id],
            exports: Vec::new(),
            compile_spec: emath_ir::CompileSpec::default(),
            about: None,
            evidence: Vec::new(),
            host: Vec::new(),
            source: Span::default(),
        });
        package
    }

    fn square_worked(given_literal: &str) -> SemanticPackage {
        let mut package = SemanticPackage::new();
        let ty = package.push_type(TypeNode::Float64);
        let x = package.push_expr(
            ExprNode::Variable(QualifiedName::single("x")),
            Span::default(),
        );
        let y_def = package.push_expr(
            ExprNode::Binary {
                operation: BinaryOp::StrictFloatMul,
                left: x,
                right: x,
            },
            Span::default(),
        );
        let given_expr = package.push_expr(
            ExprNode::Literal(Literal::Integer(given_literal.to_string())),
            Span::default(),
        );
        let mut given = BTreeMap::new();
        given.insert("x".to_string(), given_expr);
        let test_id = package.push_test(emath_ir::TestCase {
            name: "four_squared".to_string(),
            given,
            expect: None,
            source: Span::default(),
        });
        let mut definitions = BTreeMap::new();
        definitions.insert("y".to_string(), y_def);
        package.declarations.push(emath_ir::Declaration {
            id: DeclarationId(0),
            name: QualifiedName::single("Square"),
            kind: QualifiedName::single("function"),
            kind_label: "function".to_string(),
            inputs: vec![float_field("x", ty)],
            outputs: vec![float_field("y", ty)],
            state: Vec::new(),
            algebraic: Vec::new(),
            constructors: Vec::new(),
            definitions,
            invariants: Vec::new(),
            goals: Vec::new(),
            tests: vec![test_id],
            exports: Vec::new(),
            compile_spec: emath_ir::CompileSpec::default(),
            about: None,
            evidence: Vec::new(),
            host: Vec::new(),
            source: Span::default(),
        });
        package
    }

    #[test]
    fn runner_square_worked_example_computes() {
        let report = run_package(&square_worked("4"));
        assert_eq!(report.summary.tests, 1);
        assert_eq!(report.summary.computed, 1);
        assert_eq!(report.summary.passed, 0);
        assert_eq!(report.summary.failed, 0);
        let test = &report.declarations[0].tests[0];
        assert_eq!(test.verdict, TestVerdict::Computed);
        assert_eq!(test.given.get("x").cloned(), Some(Value::F64(4.0)));
        assert_eq!(test.definitions.get("y"), Some(&Value::F64(16.0)));
        assert_eq!(test.outputs.get("y"), Some(&Value::F64(16.0)));
    }

    #[test]
    fn runner_square_expect_passes() {
        let report = run_package(&square_package("9"));
        assert_eq!(report.summary.tests, 1);
        assert_eq!(report.summary.passed, 1);
        assert_eq!(report.summary.failed, 0);
        let test = &report.declarations[0].tests[0];
        assert_eq!(test.verdict, TestVerdict::Passed);
        assert_eq!(test.given.get("x").cloned(), Some(Value::F64(3.0)));
        assert_eq!(test.definitions.get("y"), Some(&Value::F64(9.0)));
        assert_eq!(test.outputs.get("y"), Some(&Value::F64(9.0)));
    }

    #[test]
    fn runner_square_expect_fails() {
        let report = run_package(&square_package("8"));
        assert_eq!(report.summary.failed, 1);
        assert!(!report.declarations[0].tests[0].verdict.expect_passed());
    }

    #[test]
    fn runner_constant_only_declaration_computes() {
        let mut package = SemanticPackage::new();
        let ty = package.push_type(TypeNode::Float64);
        let three = package.push_expr(
            ExprNode::Literal(Literal::Integer("3".to_string())),
            Span::default(),
        );
        let seven = package.push_expr(
            ExprNode::Literal(Literal::Integer("7".to_string())),
            Span::default(),
        );
        let y_def = package.push_expr(
            ExprNode::Binary {
                operation: BinaryOp::StrictFloatMul,
                left: three,
                right: seven,
            },
            Span::default(),
        );
        let y = package.push_expr(
            ExprNode::Variable(QualifiedName::single("y")),
            Span::default(),
        );
        let twenty_one = package.push_expr(
            ExprNode::Literal(Literal::Integer("21".to_string())),
            Span::default(),
        );
        let expect = package.push_expr(
            ExprNode::Binary {
                operation: BinaryOp::Equal,
                left: y,
                right: twenty_one,
            },
            Span::default(),
        );
        let test_id = package.push_test(emath_ir::TestCase {
            name: "worked".to_string(),
            given: BTreeMap::new(),
            expect: Some(expect),
            source: Span::default(),
        });
        let mut definitions = BTreeMap::new();
        definitions.insert("y".to_string(), y_def);
        package.declarations.push(emath_ir::Declaration {
            id: DeclarationId(0),
            name: QualifiedName::single("TwentyOne"),
            kind: QualifiedName::single("function"),
            kind_label: "function".to_string(),
            inputs: Vec::new(),
            outputs: vec![float_field("y", ty)],
            state: Vec::new(),
            algebraic: Vec::new(),
            constructors: Vec::new(),
            definitions,
            invariants: Vec::new(),
            goals: Vec::new(),
            tests: vec![test_id],
            exports: Vec::new(),
            compile_spec: emath_ir::CompileSpec::default(),
            about: None,
            evidence: Vec::new(),
            host: Vec::new(),
            source: Span::default(),
        });
        let report = run_package(&package);
        assert_eq!(report.summary.tests, 1);
        assert_eq!(report.summary.passed, 1);
        assert_eq!(report.summary.failed, 0);
        assert_eq!(report.summary.refused, 0);
        let test = &report.declarations[0].tests[0];
        assert!(test.given.is_empty());
        assert_eq!(test.verdict, TestVerdict::Passed);
        assert_eq!(test.definitions.get("y"), Some(&Value::F64(21.0)));
        assert_eq!(test.outputs.get("y"), Some(&Value::F64(21.0)));
    }

    fn square_no_tests() -> SemanticPackage {
        let mut package = SemanticPackage::new();
        let ty = package.push_type(TypeNode::Float64);
        let x = package.push_expr(
            ExprNode::Variable(QualifiedName::single("x")),
            Span::default(),
        );
        let y_def = package.push_expr(
            ExprNode::Binary {
                operation: BinaryOp::StrictFloatMul,
                left: x,
                right: x,
            },
            Span::default(),
        );
        let mut definitions = BTreeMap::new();
        definitions.insert("y".to_string(), y_def);
        package.declarations.push(emath_ir::Declaration {
            id: DeclarationId(0),
            name: QualifiedName::single("Square"),
            kind: QualifiedName::single("function"),
            kind_label: "function".to_string(),
            inputs: vec![float_field("x", ty)],
            outputs: vec![float_field("y", ty)],
            state: Vec::new(),
            algebraic: Vec::new(),
            constructors: Vec::new(),
            definitions,
            invariants: Vec::new(),
            goals: Vec::new(),
            tests: Vec::new(),
            exports: Vec::new(),
            compile_spec: emath_ir::CompileSpec::default(),
            about: None,
            evidence: Vec::new(),
            host: Vec::new(),
            source: Span::default(),
        });
        package
    }

    #[test]
    fn runner_zero_tests_notes() {
        let report = run_package(&square_no_tests());
        assert_eq!(report.declarations[0].tests.len(), 0);
        assert_eq!(report.declarations[0].note.as_deref(), Some(ZERO_TEST_NOTE));
        assert_eq!(report.summary.tests, 0);
    }

    #[test]
    fn runner_zero_tests_computes_when_inputs_bound() {
        let mut package = SemanticPackage::new();
        let ty = package.push_type(TypeNode::Float64);
        let two = package.push_expr(
            ExprNode::Literal(Literal::Integer("2".to_string())),
            Span::default(),
        );
        let a_var = package.push_expr(
            ExprNode::Variable(QualifiedName::single("a")),
            Span::default(),
        );
        let b_def = package.push_expr(
            ExprNode::Binary {
                operation: BinaryOp::StrictFloatMul,
                left: a_var,
                right: a_var,
            },
            Span::default(),
        );
        let mut definitions = BTreeMap::new();
        definitions.insert("a".to_string(), two);
        definitions.insert("b".to_string(), b_def);
        package.declarations.push(emath_ir::Declaration {
            id: DeclarationId(0),
            name: QualifiedName::single("Pane"),
            kind: QualifiedName::single("function"),
            kind_label: "function".to_string(),
            inputs: Vec::new(),
            outputs: vec![float_field("a", ty), float_field("b", ty)],
            state: Vec::new(),
            algebraic: Vec::new(),
            constructors: Vec::new(),
            definitions,
            invariants: Vec::new(),
            goals: Vec::new(),
            tests: Vec::new(),
            exports: Vec::new(),
            compile_spec: emath_ir::CompileSpec::default(),
            about: None,
            evidence: Vec::new(),
            host: Vec::new(),
            source: Span::default(),
        });
        let report = run_package(&package);
        assert_eq!(report.summary.tests, 1);
        assert_eq!(report.summary.computed, 1);
        let test = &report.declarations[0].tests[0];
        assert_eq!(test.name, PANE_TEST_NAME);
        assert_eq!(test.verdict, TestVerdict::Computed);
        assert_eq!(test.definitions.get("a"), Some(&Value::F64(2.0)));
        assert_eq!(test.definitions.get("b"), Some(&Value::F64(4.0)));
    }

    #[test]
    fn runner_definitions_evaluate_in_source_order_not_name_order() {
        // `z = 2` precedes `a = z * z` in the source; name order would
        // evaluate `a` first and misread `z` as an unbound input.
        let mut package = SemanticPackage::new();
        let ty = package.push_type(TypeNode::Float64);
        let file = Span::default().file;
        let two = package.push_expr(
            ExprNode::Literal(Literal::Integer("2".to_string())),
            Span::new(file, 10, 11),
        );
        let z_var = package.push_expr(
            ExprNode::Variable(QualifiedName::single("z")),
            Span::new(file, 20, 21),
        );
        let a_def = package.push_expr(
            ExprNode::Binary {
                operation: BinaryOp::StrictFloatMul,
                left: z_var,
                right: z_var,
            },
            Span::new(file, 20, 25),
        );
        let mut definitions = BTreeMap::new();
        definitions.insert("z".to_string(), two);
        definitions.insert("a".to_string(), a_def);
        package.declarations.push(emath_ir::Declaration {
            id: DeclarationId(0),
            name: QualifiedName::single("Pane"),
            kind: QualifiedName::single("function"),
            kind_label: "function".to_string(),
            inputs: Vec::new(),
            outputs: vec![float_field("a", ty), float_field("z", ty)],
            state: Vec::new(),
            algebraic: Vec::new(),
            constructors: Vec::new(),
            definitions,
            invariants: Vec::new(),
            goals: Vec::new(),
            tests: Vec::new(),
            exports: Vec::new(),
            compile_spec: emath_ir::CompileSpec::default(),
            about: None,
            evidence: Vec::new(),
            host: Vec::new(),
            source: Span::default(),
        });
        let report = run_package(&package);
        assert_eq!(report.summary.computed, 1, "{report:?}");
        let test = &report.declarations[0].tests[0];
        assert_eq!(test.verdict, TestVerdict::Computed);
        assert_eq!(test.definitions.get("z"), Some(&Value::F64(2.0)));
        assert_eq!(test.definitions.get("a"), Some(&Value::F64(4.0)));
    }

    #[test]
    fn runner_pane_given_computes_and_missing_refuses() {
        let package = square_no_tests();
        let mut given = BTreeMap::new();
        given.insert("x".to_string(), Value::F64(5.0));
        let report = run_package_with_given(&package, Some(&given));
        assert_eq!(report.summary.computed, 1);
        let test = &report.declarations[0].tests[0];
        assert_eq!(test.name, PANE_TEST_NAME);
        assert_eq!(test.definitions.get("y"), Some(&Value::F64(25.0)));

        let empty = BTreeMap::new();
        let refused = run_package_with_given(&package, Some(&empty));
        assert_eq!(refused.summary.refused, 1);
        match &refused.declarations[0].tests[0].verdict {
            TestVerdict::LoweringRefused { detail } => {
                assert!(detail.contains("`x`"), "{detail}");
            }
            other => panic!("expected missing-input refusal, got {other:?}"),
        }
    }

    #[test]
    fn runner_constructor_refuses_false_require() {
        let mut package = SemanticPackage::new();
        let ty = package.push_type(TypeNode::Float64);
        let scale = package.push_expr(
            ExprNode::Variable(QualifiedName::single("scale")),
            Span::default(),
        );
        let zero = package.push_expr(
            ExprNode::Literal(Literal::Integer("0".to_string())),
            Span::default(),
        );
        let require = package.push_expr(
            ExprNode::Binary {
                operation: BinaryOp::GreaterEqual,
                left: scale,
                right: zero,
            },
            Span::default(),
        );
        let neg = package.push_expr(
            ExprNode::Literal(Literal::Integer("-1".to_string())),
            Span::default(),
        );
        let x = package.push_expr(
            ExprNode::Literal(Literal::Integer("1".to_string())),
            Span::default(),
        );
        let expect = package.push_expr(ExprNode::Literal(Literal::Bool(true)), Span::default());
        let mut given = BTreeMap::new();
        given.insert("scale".to_string(), neg);
        given.insert("x".to_string(), x);
        let test_id = package.push_test(emath_ir::TestCase {
            name: "bad_scale".to_string(),
            given,
            expect: Some(expect),
            source: Span::default(),
        });
        let mut assignments = BTreeMap::new();
        assignments.insert("scale".to_string(), scale);
        package.declarations.push(emath_ir::Declaration {
            id: DeclarationId(0),
            name: QualifiedName::single("Policy"),
            kind: QualifiedName::single("policy"),
            kind_label: "policy".to_string(),
            inputs: vec![float_field("x", ty)],
            outputs: Vec::new(),
            state: vec![float_field("scale", ty)],
            algebraic: Vec::new(),
            constructors: vec![Constructor {
                name: "new".to_string(),
                parameters: vec![float_field("scale", ty)],
                preconditions: vec![require],
                assignments,
                postconditions: Vec::new(),
                defaults: BTreeMap::new(),
                error_type: None,
                is_public: true,
                source: Span::default(),
            }],
            definitions: BTreeMap::new(),
            invariants: Vec::new(),
            goals: Vec::new(),
            tests: vec![test_id],
            exports: Vec::new(),
            compile_spec: emath_ir::CompileSpec::default(),
            about: None,
            evidence: Vec::new(),
            host: Vec::new(),
            source: Span::default(),
        });
        let report = run_package(&package);
        assert_eq!(report.summary.refused, 1);
        match &report.declarations[0].tests[0].verdict {
            TestVerdict::ConstructorRefused { obligation } => {
                assert!(obligation.contains("scale"), "{obligation}");
            }
            other => panic!("expected constructor refused, got {other:?}"),
        }
    }
}
