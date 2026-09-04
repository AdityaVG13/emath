//! emath-s9w1m: strict-f64 emitter call-path builtins.
//!
//! Failure-first contract: `abs`/`min`/`max`/`sqrt`/`atan2` written as
//! CALLS (the shape the probability/geometry/analysis lanes emit) must
//! lower through the `BuiltinId` registry and run on the reference VM —
//! same documented contracts as the unary/binary SIR paths, never the
//! `unknown function ... strict-f64 subset` catch-all. Unknown names
//! keep the typed fence; arity mismatches refuse typed.

use std::collections::BTreeMap;

use emath_core::{QualifiedName, Span};
use emath_exec_ir::interp::Value;
use emath_exec_ir::runner::{TestVerdict, run_package};
use emath_ir::{DeclarationId, ExprNode, Field, Literal, SemanticPackage, TypeNode, Visibility};

fn float_field(name: &str, ty: emath_ir::TypeId) -> Field {
    Field {
        name: name.to_string(),
        ty,
        visibility: Visibility::Public,
        source: Span::default(),
    }
}

fn var(name: &str) -> ExprNode {
    ExprNode::Variable(QualifiedName::single(name))
}

fn literal(value: f64) -> ExprNode {
    ExprNode::Literal(Literal::FloatBits(value.to_bits()))
}

fn call(name: &str, args: Vec<emath_ir::ExprId>) -> ExprNode {
    ExprNode::Call {
        function: QualifiedName::single(name),
        arguments: args,
    }
}

/// Package with definitions `y = abs(x)`, `m = min(a, b)`,
/// `M = max(a, b)`, `s = sqrt(x)`, `t = atan2(a, b)` and one worked
/// example binding `x`, `a`, `b` to the given values.
fn builtins_package(x_given: f64, a_given: f64, b_given: f64) -> SemanticPackage {
    let mut package = SemanticPackage::new();
    let ty = package.push_type(TypeNode::Float64);
    let x_var = package.push_expr(var("x"), Span::default());
    let a_var = package.push_expr(var("a"), Span::default());
    let b_var = package.push_expr(var("b"), Span::default());
    let abs_def = package.push_expr(call("abs", vec![x_var]), Span::default());
    let min_def = package.push_expr(
        call("min", vec![a_var.clone(), b_var.clone()]),
        Span::default(),
    );
    let max_def = package.push_expr(
        call("max", vec![a_var.clone(), b_var.clone()]),
        Span::default(),
    );
    let sqrt_def = package.push_expr(call("sqrt", vec![x_var.clone()]), Span::default());
    let atan2_def = package.push_expr(call("atan2", vec![a_var, b_var]), Span::default());
    let mut definitions = BTreeMap::new();
    definitions.insert("y".to_string(), abs_def);
    definitions.insert("m".to_string(), min_def);
    definitions.insert("M".to_string(), max_def);
    definitions.insert("s".to_string(), sqrt_def);
    definitions.insert("t".to_string(), atan2_def);

    let x_lit = package.push_expr(literal(x_given), Span::default());
    let a_lit = package.push_expr(literal(a_given), Span::default());
    let b_lit = package.push_expr(literal(b_given), Span::default());
    let mut given = BTreeMap::new();
    given.insert("x".to_string(), x_lit);
    given.insert("a".to_string(), a_lit);
    given.insert("b".to_string(), b_lit);
    let test_id = package.push_test(emath_ir::TestCase {
        name: "worked".to_string(),
        given,
        expect: None,
        source: Span::default(),
    });
    package.declarations.push(emath_ir::Declaration {
        id: DeclarationId(0),
        name: QualifiedName::single("Builtins"),
        kind: QualifiedName::single("function"),
        kind_label: "function".to_string(),
        inputs: vec![
            float_field("x", ty),
            float_field("a", ty),
            float_field("b", ty),
        ],
        outputs: vec![
            float_field("y", ty),
            float_field("m", ty),
            float_field("M", ty),
            float_field("s", ty),
            float_field("t", ty),
        ],
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
fn emitter_registry_calls_lower_and_run() {
    let report = run_package(&builtins_package(9.0, 2.0, 5.0));
    let test = &report.declarations[0].tests[0];
    assert_eq!(test.verdict, TestVerdict::Computed);
    assert_eq!(test.outputs.get("y"), Some(&Value::F64(9.0)));
    assert_eq!(test.outputs.get("m"), Some(&Value::F64(2.0)));
    assert_eq!(test.outputs.get("M"), Some(&Value::F64(5.0)));
    assert_eq!(test.outputs.get("s"), Some(&Value::F64(3.0)));
    let Value::F64(t) = test.outputs.get("t").expect("atan2 computes") else {
        panic!("atan2 must compute a scalar");
    };
    assert!((t - 2f64.atan2(5.0)).abs() <= 1e-15, "t = {t}");
}

#[test]
fn emitter_sqrt_negative_is_documented_nan_not_a_crash() {
    // The strict-f64 sqrt contract (builtin.rs / program.rs): a negative
    // domain argument evaluates to NaN — a value, never a crash and
    // never a fabricated finite number.
    let report = run_package(&builtins_package(-3.0, 2.0, 5.0));
    let test = &report.declarations[0].tests[0];
    assert_eq!(test.verdict, TestVerdict::Computed);
    assert_eq!(test.outputs.get("y"), Some(&Value::F64(3.0)));
    assert_eq!(test.outputs.get("m"), Some(&Value::F64(2.0)));
    assert_eq!(test.outputs.get("M"), Some(&Value::F64(5.0)));
    assert_eq!(test.outputs.get("s"), Some(&Value::F64(f64::NAN)));
}

fn unknown_call_package(function: &str) -> SemanticPackage {
    let mut package = SemanticPackage::new();
    let ty = package.push_type(TypeNode::Float64);
    let a_var = package.push_expr(var("a"), Span::default());
    let b_var = package.push_expr(var("b"), Span::default());
    let definition = package.push_expr(call(function, vec![a_var, b_var]), Span::default());
    let mut definitions = BTreeMap::new();
    definitions.insert("z".to_string(), definition);
    let a_lit = package.push_expr(literal(2.0), Span::default());
    let b_lit = package.push_expr(literal(5.0), Span::default());
    let mut given = BTreeMap::new();
    given.insert("a".to_string(), a_lit);
    given.insert("b".to_string(), b_lit);
    let test_id = package.push_test(emath_ir::TestCase {
        name: "worked".to_string(),
        given,
        expect: None,
        source: Span::default(),
    });
    package.declarations.push(emath_ir::Declaration {
        id: DeclarationId(0),
        name: QualifiedName::single("Unknown"),
        kind: QualifiedName::single("function"),
        kind_label: "function".to_string(),
        inputs: vec![float_field("a", ty), float_field("b", ty)],
        outputs: vec![float_field("z", ty)],
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
fn unknown_function_keeps_typed_fence() {
    let report = run_package(&unknown_call_package("frobnicate"));
    let test = &report.declarations[0].tests[0];
    assert!(!matches!(test.verdict, TestVerdict::Symbolic { .. }));
    match &test.verdict {
        TestVerdict::LoweringRefused { detail } => {
            assert!(
                detail.contains("unknown function `frobnicate` in strict-f64 subset"),
                "{detail}"
            );
        }
        other => panic!("expected the typed fence, got {other:?}"),
    }
}

#[test]
fn registry_arity_mismatch_refuses_typed() {
    let mut package = SemanticPackage::new();
    let ty = package.push_type(TypeNode::Float64);
    let a_var = package.push_expr(var("a"), Span::default());
    let b_var = package.push_expr(var("b"), Span::default());
    let bad = package.push_expr(call("abs", vec![a_var, b_var]), Span::default());
    let mut definitions = BTreeMap::new();
    definitions.insert("z".to_string(), bad);
    let a_lit = package.push_expr(literal(2.0), Span::default());
    let b_lit = package.push_expr(literal(5.0), Span::default());
    let mut given = BTreeMap::new();
    given.insert("a".to_string(), a_lit);
    given.insert("b".to_string(), b_lit);
    let test_id = package.push_test(emath_ir::TestCase {
        name: "bad_arity".to_string(),
        given,
        expect: None,
        source: Span::default(),
    });
    package.declarations.push(emath_ir::Declaration {
        id: DeclarationId(0),
        name: QualifiedName::single("BadArity"),
        kind: QualifiedName::single("function"),
        kind_label: "function".to_string(),
        inputs: vec![float_field("a", ty), float_field("b", ty)],
        outputs: vec![float_field("z", ty)],
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
    match &report.declarations[0].tests[0].verdict {
        TestVerdict::LoweringRefused { detail } => {
            assert!(detail.contains("`abs` expects 1 operand(s)"), "{detail}");
        }
        other => panic!("expected typed arity refusal, got {other:?}"),
    }
}
