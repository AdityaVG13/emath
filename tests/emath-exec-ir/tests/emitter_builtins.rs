//! Strict-f64 builtin call-path contracts.
//!
//! Failure-first: the executable emitter lowers the strict-f64 surface
//! through `Unary`/`Binary` builtin nodes (`abs`/`min`/`max`/`sqrt`/
//! `atan2`), which must run on the reference VM with the documented
//! contracts — `sqrt` of a negative is IEEE NaN, never a crash and never
//! a fabricated finite number. Raw NAMED math calls reaching executable
//! lowering (a shape admission can no longer produce) are a typed
//! refusal, and unknown names / bad arities keep typed fences — never a
//! panic, never a silent value.

use std::collections::BTreeMap;

use emath_core::{QualifiedName, Span};
use emath_exec_ir::interp::Value;
use emath_exec_ir::runner::{TestVerdict, run_package};
use emath_ir::{
    BinaryOp, DeclarationId, ExprNode, Field, Literal, SemanticPackage, TypeNode, UnaryOp,
    Visibility,
};
use emath_test_harness::Probe;

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

fn unary(operation: UnaryOp, value: emath_ir::ExprId) -> ExprNode {
    ExprNode::Unary { operation, value }
}

fn binary(operation: BinaryOp, left: emath_ir::ExprId, right: emath_ir::ExprId) -> ExprNode {
    ExprNode::Binary { operation, left, right }
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
    let abs_def = package.push_expr(unary(UnaryOp::Abs, x_var), Span::default());
    let min_def = package.push_expr(binary(BinaryOp::Min, a_var, b_var.clone()), Span::default());
    let max_def = package.push_expr(binary(BinaryOp::Max, a_var, b_var), Span::default());
    let sqrt_def = package.push_expr(unary(UnaryOp::Sqrt, x_var), Span::default());
    let atan2_def = package.push_expr(binary(BinaryOp::Atan2, a_var, b_var), Span::default());
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
        inputs: vec![float_field("x", ty), float_field("a", ty), float_field("b", ty)],
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

fn builtin_family(ph: &mut Probe) {
    ph.case("builtins_lower_and_run", |ph| {
        let report = run_package(&builtins_package(9.0, 2.0, 5.0));
        let test = &report.declarations[0].tests[0];
        eq_ref(ph, "verdict", &(test.verdict), &(TestVerdict::Computed));
        eq_ref(ph, "abs", &(test.outputs.get("y")), &(Some(&Value::F64(9.0))));
        eq_ref(ph, "min", &(test.outputs.get("m")), &(Some(&Value::F64(2.0))));
        eq_ref(ph, "max", &(test.outputs.get("M")), &(Some(&Value::F64(5.0))));
        eq_ref(ph, "sqrt", &(test.outputs.get("s")), &(Some(&Value::F64(3.0))));
        let Value::F64(t) = *test.outputs.get("t").expect("atan2 computes") else {
            panic!("atan2 must compute a scalar");
        };
        ph.close("atan2-value", t, 2.0_f64.atan2(5.0), 1e-15);
    });
    // sqrt of a negative domain argument is NaN — a value, never a crash
    // and never a fabricated finite number.
    ph.case("sqrt_negative_is_nan", |ph| {
        let report = run_package(&builtins_package(-3.0, 2.0, 5.0));
        let test = &report.declarations[0].tests[0];
        eq_ref(ph, "verdict", &(test.verdict), &(TestVerdict::Computed));
        eq_ref(ph, "abs", &(test.outputs.get("y")), &(Some(&Value::F64(3.0))));
        eq_ref(ph, "min", &(test.outputs.get("m")), &(Some(&Value::F64(2.0))));
        eq_ref(ph, "max", &(test.outputs.get("M")), &(Some(&Value::F64(5.0))));
        eq_ref(ph, "sqrt-nan", &(test.outputs.get("s")), &(Some(&Value::F64(f64::NAN))));
    });
    // Raw named math calls are a retired shape: admission resolves
    // FeatureID applications, so a call that still reaches executable
    // lowering must refuse typed — never compute, never turn symbolic,
    // never panic.
    ph.case("named_call_is_typed_refusal", |ph| {
        let report = run_package(&unknown_call_package("frobnicate"));
        let test = &report.declarations[0].tests[0];
        match &test.verdict {
            TestVerdict::LoweringRefused { detail } => {
                ph.contains("names-function", detail, "`frobnicate`");
                ph.contains("names-shape", detail, "legacy named call");
            }
            other => { ph.fail("typed-fence", format!("expected typed refusal, got {other:?}")); }
        }
    });
    ph.case("arity_mismatch_is_typed_refusal", |ph| {
        let report = run_package(&unknown_call_package("abs"));
        let test = &report.declarations[0].tests[0];
        match &test.verdict {
            TestVerdict::LoweringRefused { detail } => {
                ph.contains("names-function", detail, "`abs`");
                ph.contains("names-shape", detail, "legacy named call");
            }
            other => { ph.fail("typed-fence", format!("expected typed refusal, got {other:?}")); }
        }
    });
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

/// assert_eq/assert_ne semantics over references: borrows both operands
/// (like the macros) and allows PartialEq between distinct types.
fn eq_ref<T: ?Sized + std::fmt::Debug, U: ?Sized + std::fmt::Debug>(
    ph: &mut Probe,
    name: impl Into<String>,
    actual: &T,
    expected: &U,
) where
    T: PartialEq<U>,
{
    ph.demand(name, actual == expected, format!("expected {expected:?}, got {actual:?}"));
}

#[test]
fn emitter_builtin_contracts() {
    let mut ph = Probe::new(
        "strict-f64 builtins run through unary/binary lowering with IEEE NaN sqrt; named math calls keep the typed fence",
    );
    builtin_family(&mut ph);
    ph.finish();
}
