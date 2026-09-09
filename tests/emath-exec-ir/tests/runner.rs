//! Declaration-runner tests migrated from the in-crate `#[cfg(test)]`
//! module: the runner entry points (`run_package`, `run_package_with_given`)
//! and the `SemanticPackage` builder are public crate surface, so these
//! exercise the API exactly as an embedder would.

use std::collections::BTreeMap;

use emath_core::{QualifiedName, Span};
use emath_exec_ir::interp::Value;
use emath_exec_ir::runner::{PANE_TEST_NAME, TestVerdict, run_package, run_package_with_given};
use emath_ir::{
    BinaryOp, Constructor, DeclarationId, ExprNode, Field, Literal, SemanticPackage, TypeNode,
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

fn ne_ref<T: ?Sized + std::fmt::Debug, U: ?Sized + std::fmt::Debug>(
    ph: &mut Probe,
    name: impl Into<String>,
    actual: &T,
    unexpected: &U,
) where
    T: PartialEq<U>,
{
    ph.demand(name, actual != unexpected, format!("got forbidden value {unexpected:?}"));
}

fn runner_square_worked_example_computes(ph: &mut Probe) {
    ph.case("runner_square_worked_example_computes", |ph| {
    let report = run_package(&square_worked("4"));
    eq_ref(ph, "1", &(report.summary.tests), &( 1));
    eq_ref(ph, "2", &(report.summary.computed), &( 1));
    eq_ref(ph, "3", &(report.summary.passed), &( 0));
    eq_ref(ph, "4", &(report.summary.failed), &( 0));
    let test = &report.declarations[0].tests[0];
    eq_ref(ph, "5", &(test.verdict), &( TestVerdict::Computed));
    eq_ref(ph, "6", &(test.given.get("x").cloned()), &( Some(Value::F64(4.0))));
    eq_ref(ph, "7", &(test.definitions.get("y")), &( Some(&Value::F64(16.0))));
    eq_ref(ph, "8", &(test.outputs.get("y")), &( Some(&Value::F64(16.0))));

    });
}

fn runner_square_expect_passes(ph: &mut Probe) {
    ph.case("runner_square_expect_passes", |ph| {
    let report = run_package(&square_package("9"));
    eq_ref(ph, "1", &(report.summary.tests), &( 1));
    eq_ref(ph, "2", &(report.summary.passed), &( 1));
    eq_ref(ph, "3", &(report.summary.failed), &( 0));
    let test = &report.declarations[0].tests[0];
    eq_ref(ph, "4", &(test.verdict), &( TestVerdict::Passed));
    eq_ref(ph, "5", &(test.given.get("x").cloned()), &( Some(Value::F64(3.0))));
    eq_ref(ph, "6", &(test.definitions.get("y")), &( Some(&Value::F64(9.0))));
    eq_ref(ph, "7", &(test.outputs.get("y")), &( Some(&Value::F64(9.0))));

    });
}

fn runner_square_expect_fails(ph: &mut Probe) {
    ph.case("runner_square_expect_fails", |ph| {
    let report = run_package(&square_package("8"));
    eq_ref(ph, "1", &(report.summary.failed), &( 1));
    ph.demand("2", !report.declarations[0].tests[0].verdict.expect_passed(), "assertion failed: !report.declarations[0].tests[0].verdict.expect_passed()");

    });
}

fn runner_constant_only_declaration_computes(ph: &mut Probe) {
    ph.case("runner_constant_only_declaration_computes", |ph| {
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
    eq_ref(ph, "1", &(report.summary.tests), &( 1));
    eq_ref(ph, "2", &(report.summary.passed), &( 1));
    eq_ref(ph, "3", &(report.summary.failed), &( 0));
    eq_ref(ph, "4", &(report.summary.refused), &( 0));
    let test = &report.declarations[0].tests[0];
    ph.demand("5", test.given.is_empty(), "assertion failed: test.given.is_empty()");
    eq_ref(ph, "6", &(test.verdict), &( TestVerdict::Passed));
    eq_ref(ph, "7", &(test.definitions.get("y")), &( Some(&Value::F64(21.0))));
    eq_ref(ph, "8", &(test.outputs.get("y")), &( Some(&Value::F64(21.0))));

    });
}

fn runner_unbound_declaration_returns_symbolic_form_with_label(ph: &mut Probe) {
    ph.case("runner_unbound_declaration_returns_symbolic_form_with_label", |ph| {
    // nothing-returns-nothing: the old zero-test note (no run at all)
    // is replaced by one labeled `_pane` run that carries the symbolic
    // form of the definitions no world can evaluate.
    let report = run_package(&square_no_tests());
    eq_ref(ph, "1", &(report.declarations[0].tests.len()), &( 1));
    eq_ref(ph, "2", &(report.declarations[0].note), &( None));
    eq_ref(ph, "3", &(report.summary.symbolic), &( 1));
    let test = &report.declarations[0].tests[0];
    eq_ref(ph, "4", &(test.name), &( PANE_TEST_NAME));
    match &test.verdict {
        TestVerdict::Symbolic {
            label,
            forms,
            holes,
        } => {
            eq_ref(ph, "5", &(*label), &( "symbolic-only"));
            eq_ref(ph, "6", &(forms.get("y").map(String::as_str)), &( Some("x * x")));
            eq_ref(ph, "7", &(holes.get("x").map(String::as_str)), &( Some("Float64")));
        }
        other => panic!("expected a labeled symbolic verdict, got {other:?}"),
    }
    eq_ref(ph, "8", &(test.definitions.get("y")), &( None));

    });
}

fn runner_zero_tests_computes_when_inputs_bound(ph: &mut Probe) {
    ph.case("runner_zero_tests_computes_when_inputs_bound", |ph| {
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
    eq_ref(ph, "1", &(report.summary.tests), &( 1));
    eq_ref(ph, "2", &(report.summary.computed), &( 1));
    let test = &report.declarations[0].tests[0];
    eq_ref(ph, "3", &(test.name), &( PANE_TEST_NAME));
    eq_ref(ph, "4", &(test.verdict), &( TestVerdict::Computed));
    eq_ref(ph, "5", &(test.definitions.get("a")), &( Some(&Value::F64(2.0))));
    eq_ref(ph, "6", &(test.definitions.get("b")), &( Some(&Value::F64(4.0))));

    });
}

fn runner_definitions_evaluate_in_source_order_not_name_order(ph: &mut Probe) {
    ph.case("runner_definitions_evaluate_in_source_order_not_name_order", |ph| {
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
    eq_ref(ph, format!( "{report:?}"), &(report.summary.computed), &( 1));
    let test = &report.declarations[0].tests[0];
    eq_ref(ph, "2", &(test.verdict), &( TestVerdict::Computed));
    eq_ref(ph, "3", &(test.definitions.get("z")), &( Some(&Value::F64(2.0))));
    eq_ref(ph, "4", &(test.definitions.get("a")), &( Some(&Value::F64(4.0))));

    });
}

fn runner_pane_given_computes_and_empty_binding_is_symbolic(ph: &mut Probe) {
    ph.case("runner_pane_given_computes_and_empty_binding_is_symbolic", |ph| {
    let package = square_no_tests();
    let mut given = BTreeMap::new();
    given.insert("x".to_string(), Value::F64(5.0));
    let report = run_package_with_given(&package, Some(&given));
    eq_ref(ph, "1", &(report.summary.computed), &( 1));
    let test = &report.declarations[0].tests[0];
    eq_ref(ph, "2", &(test.name), &( PANE_TEST_NAME));
    eq_ref(ph, "3", &(test.definitions.get("y")), &( Some(&Value::F64(25.0))));

    // An empty pane binding map is still a run: no world can evaluate,
    // so the labeled symbolic form comes back instead of a refusal.
    let empty = BTreeMap::new();
    let symbolic = run_package_with_given(&package, Some(&empty));
    eq_ref(ph, "4", &(symbolic.summary.symbolic), &( 1));
    match &symbolic.declarations[0].tests[0].verdict {
        TestVerdict::Symbolic { label, forms, .. } => {
            eq_ref(ph, "5", &(*label), &( "symbolic-only"));
            eq_ref(ph, "6", &(forms.get("y").map(String::as_str)), &( Some("x * x")));
        }
        other => panic!("expected labeled symbolic verdict, got {other:?}"),
    }

    });
}

fn runner_named_test_missing_input_still_refuses_typed(ph: &mut Probe) {
    ph.case("runner_named_test_missing_input_still_refuses_typed", |ph| {
    // A named example that omits a required input is a program bug, not
    // a world gap: the typed refusal must survive the symbolic fallback.
    let mut package = square_package("9");
    package.tests[0].given.clear();
    let report = run_package(&package);
    eq_ref(ph, "1", &(report.summary.refused), &( 1));
    let test = &report.declarations[0].tests[0];
    ph.demand("2", !matches!(test.verdict, TestVerdict::Symbolic { .. }), "assertion failed: !matches!(test.verdict, TestVerdict::Symbolic { .. })");
    match &test.verdict {
        TestVerdict::LoweringRefused { detail } => {
            ph.demand("3", detail.contains("`x`"), format!( "{detail}"));
        }
        other => panic!("expected typed refusal, got {other:?}"),
    }

    });
}

fn runner_constructor_refuses_false_require(ph: &mut Probe) {
    ph.case("runner_constructor_refuses_false_require", |ph| {
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
    eq_ref(ph, "1", &(report.summary.refused), &( 1));
    match &report.declarations[0].tests[0].verdict {
        TestVerdict::ConstructorRefused { obligation } => {
            ph.demand("2", obligation.contains("scale"), format!( "{obligation}"));
        }
        other => panic!("expected constructor refused, got {other:?}"),
    }

    });
}

// Scalar source fixtures provide admitted packages. The mutations below test
// vector residuals through the public API, not new source-syntax admission.
const SCALAR_ALG_SRC: &str = "emath model ScalarAlg:
    algebraic:
        z: Float64
    state:
        q: Float64
    equations:
        z == 1.0
        der(q) = 0.0
";
const SCALAR_RATE_SRC: &str = "emath model ScalarRate:
    state:
        v: Float64
    equations:
        der(v) = 1.0
";
const SHAPE_MIX_SRC: &str = "emath model ShapeMix:
    state:
        a: Vector[2]
        b: Vector[2]
    algebraic:
        z: Float64
    equations:
        z == 1.0
        der(a) = [1.0, 2.0]
        der(b) = [3.0, 4.0]
";

#[test]
fn newton_map_count_and_shape() {
    emath_test_harness::boot();
    let mut ph = Probe::new(
        "map Newton count uses scalar widths, singleton vectors stay vectors, residual rate shapes refuse mixing",
    );
    ph.case("algebraic-vector-2-api", |ph| {
        let mut result =
            emath_test_harness::Source::from_str("scalar-alg", SCALAR_ALG_SRC).must_admit(&mut *ph);
        if result.diagnostics.has_errors() || result.package.declarations.is_empty() {
            return;
        }
        let vec2_ty = result.package.push_type(emath_ir::TypeNode::Vector {
            element: Box::new(emath_ir::TypeNode::Float64),
            extent: Some(emath_ir::Extent::Fixed(2)),
        });
        result.package.declarations[0].algebraic[0].ty = vec2_ty;
        let z_a = result.package.push_expr(
            ExprNode::Variable(QualifiedName::single("z")),
            Span::default(),
        );
        let z_b = result.package.push_expr(
            ExprNode::Variable(QualifiedName::single("z")),
            Span::default(),
        );
        let i0 = result.package.push_expr(
            ExprNode::Literal(Literal::Integer("0".to_string())),
            Span::default(),
        );
        let i1 = result.package.push_expr(
            ExprNode::Literal(Literal::Integer("1".to_string())),
            Span::default(),
        );
        let z0 = result.package.push_expr(
            ExprNode::Index {
                value: z_a,
                indices: vec![i0],
            },
            Span::default(),
        );
        let z1 = result.package.push_expr(
            ExprNode::Index {
                value: z_b,
                indices: vec![i1],
            },
            Span::default(),
        );
        let one = result.package.push_expr(
            ExprNode::Literal(Literal::Integer("1".to_string())),
            Span::default(),
        );
        let two = result.package.push_expr(
            ExprNode::Literal(Literal::Integer("2".to_string())),
            Span::default(),
        );
        let s0 = result.package.push_expr(
            ExprNode::Binary {
                operation: BinaryOp::StrictFloatSub,
                left: z0,
                right: one,
            },
            Span::default(),
        );
        let s1 = result.package.push_expr(
            ExprNode::Binary {
                operation: BinaryOp::StrictFloatSub,
                left: z1,
                right: two,
            },
            Span::default(),
        );
        let vec_expr = result
            .package
            .push_expr(ExprNode::Vector(vec![s0, s1]), Span::default());
        let did = result.package.declarations[0].id;
        if let Some(rs) = result.package.residuals.get_mut(&did) {
            rs[0].expr = vec_expr;
            rs[0].components = 2;
        }
        let decl = &result.package.declarations[0];
        let mut state = BTreeMap::new();
        state.insert("q".to_string(), Value::F64(0.0));
        state.insert("z".to_string(), Value::Vector(vec![0.0, 0.0]));
        let mut inputs = BTreeMap::new();
        inputs.insert("z".to_string(), Value::Vector(vec![0.0, 0.0]));
        match emath_exec_ir::step_continuous_values(
            &result.package,
            decl,
            &inputs,
            &state,
            0.1,
            emath_exec_ir::StepMethod::Rk45,
        ) {
            Ok(next) => match next.get("z") {
                Some(Value::Vector(v)) => {
                    ph.demand("len", v.len() == 2, format!("expected len 2, got {v:?}"));
                    if v.len() == 2 {
                        ph.close("z0", v[0], 1.0, 1e-9);
                        ph.close("z1", v[1], 2.0, 1e-9);
                    }
                }
                other => {
                    ph.fail("kind", format!("expected Vector z, got {other:?}"));
                }
            },
            Err(error) => {
                ph.fail(
                    "solves",
                    format!("vector-2 algebraic must solve, got {error}"),
                );
            }
        }
    });
    ph.case("rate-vector-2-api", |ph| {
        let mut result = emath_test_harness::Source::from_str("scalar-rate", SCALAR_RATE_SRC)
            .must_admit(&mut *ph);
        if result.diagnostics.has_errors() || result.package.declarations.is_empty() {
            return;
        }
        let vec2_ty = result.package.push_type(emath_ir::TypeNode::Vector {
            element: Box::new(emath_ir::TypeNode::Float64),
            extent: Some(emath_ir::Extent::Fixed(2)),
        });
        result.package.declarations[0].state[0].ty = vec2_ty;
        let r_a = result.package.push_expr(
            ExprNode::Variable(QualifiedName::single("__rate_v")),
            Span::default(),
        );
        let r_b = result.package.push_expr(
            ExprNode::Variable(QualifiedName::single("__rate_v")),
            Span::default(),
        );
        let i0 = result.package.push_expr(
            ExprNode::Literal(Literal::Integer("0".to_string())),
            Span::default(),
        );
        let i1 = result.package.push_expr(
            ExprNode::Literal(Literal::Integer("1".to_string())),
            Span::default(),
        );
        let c0 = result.package.push_expr(
            ExprNode::Index {
                value: r_a,
                indices: vec![i0],
            },
            Span::default(),
        );
        let c1 = result.package.push_expr(
            ExprNode::Index {
                value: r_b,
                indices: vec![i1],
            },
            Span::default(),
        );
        let one = result.package.push_expr(
            ExprNode::Literal(Literal::Integer("1".to_string())),
            Span::default(),
        );
        let two = result.package.push_expr(
            ExprNode::Literal(Literal::Integer("2".to_string())),
            Span::default(),
        );
        let s0 = result.package.push_expr(
            ExprNode::Binary {
                operation: BinaryOp::StrictFloatSub,
                left: c0,
                right: one,
            },
            Span::default(),
        );
        let s1 = result.package.push_expr(
            ExprNode::Binary {
                operation: BinaryOp::StrictFloatSub,
                left: c1,
                right: two,
            },
            Span::default(),
        );
        let vec_expr = result
            .package
            .push_expr(ExprNode::Vector(vec![s0, s1]), Span::default());
        let did = result.package.declarations[0].id;
        result.package.declarations[0].definitions.remove("der_v");
        result.package.residuals.insert(
            did,
            vec![emath_ir::ModelResidual {
                expr: vec_expr,
                components: 2,
                algebraic: vec![],
                rates: vec!["v".to_string()],
            }],
        );
        let decl = &result.package.declarations[0];
        let mut state = BTreeMap::new();
        state.insert("v".to_string(), Value::Vector(vec![0.0, 0.0]));
        match emath_exec_ir::step_continuous_values(
            &result.package,
            decl,
            &BTreeMap::new(),
            &state,
            0.1,
            emath_exec_ir::StepMethod::Rk45,
        ) {
            Ok(next) => match next.get("v") {
                Some(Value::Vector(v)) => {
                    ph.demand("len", v.len() == 2, format!("expected len 2, got {v:?}"));
                    if v.len() == 2 {
                        ph.close("v0", v[0], 0.1, 1e-9);
                        ph.close("v1", v[1], 0.2, 1e-9);
                    }
                }
                other => {
                    ph.fail("kind", format!("expected Vector v, got {other:?}"));
                }
            },
            Err(error) => {
                ph.fail("solves", format!("vector-2 rate must solve, got {error}"));
            }
        }
    });
    ph.case("singleton-vector-1-api", |ph| {
        let mut result =
            emath_test_harness::Source::from_str("scalar-alg", SCALAR_ALG_SRC).must_admit(&mut *ph);
        if result.diagnostics.has_errors() || result.package.declarations.is_empty() {
            return;
        }
        let vec1_ty = result.package.push_type(emath_ir::TypeNode::Vector {
            element: Box::new(emath_ir::TypeNode::Float64),
            extent: Some(emath_ir::Extent::Fixed(1)),
        });
        result.package.declarations[0].algebraic[0].ty = vec1_ty;
        let z_var = result.package.push_expr(
            ExprNode::Variable(QualifiedName::single("z")),
            Span::default(),
        );
        let idx = result.package.push_expr(
            ExprNode::Literal(Literal::Integer("0".to_string())),
            Span::default(),
        );
        let z0 = result.package.push_expr(
            ExprNode::Index {
                value: z_var,
                indices: vec![idx],
            },
            Span::default(),
        );
        let five = result.package.push_expr(
            ExprNode::Literal(Literal::Integer("5".to_string())),
            Span::default(),
        );
        let sub = result.package.push_expr(
            ExprNode::Binary {
                operation: BinaryOp::StrictFloatSub,
                left: z0,
                right: five,
            },
            Span::default(),
        );
        let did = result.package.declarations[0].id;
        let residual = result
            .package
            .push_expr(ExprNode::Vector(vec![sub]), Span::default());
        if let Some(rs) = result.package.residuals.get_mut(&did) {
            rs[0].expr = residual;
            rs[0].components = 1;
        }
        let decl = &result.package.declarations[0];
        let mut state = BTreeMap::new();
        state.insert("q".to_string(), Value::F64(0.0));
        state.insert("z".to_string(), Value::Vector(vec![0.0]));
        let mut inputs = BTreeMap::new();
        inputs.insert("z".to_string(), Value::Vector(vec![0.0]));
        for (label, method) in [
            ("euler", emath_exec_ir::StepMethod::Euler),
            ("rk45", emath_exec_ir::StepMethod::Rk45),
        ] {
            match emath_exec_ir::step_continuous_values(
                &result.package,
                decl,
                &inputs,
                &state,
                0.1,
                method,
            ) {
                Ok(next) => match next.get("z") {
                    Some(Value::Vector(v)) => {
                        ph.demand(
                            format!("{label}-len"),
                            v.len() == 1,
                            format!("expected len 1, got {v:?}"),
                        );
                        if v.len() == 1 {
                            ph.close(format!("{label}-value"), v[0], 5.0, 1e-9);
                        }
                    }
                    Some(Value::F64(scalar)) => {
                        ph.fail(
                            format!("{label}-kind"),
                            format!("Vector[1] must stay Vector, got F64({scalar})"),
                        );
                    }
                    other => {
                        ph.fail(
                            format!("{label}-kind"),
                            format!("expected Vector z, got {other:?}"),
                        );
                    }
                },
                Err(error) => {
                    ph.fail(
                        format!("{label}-solves"),
                        format!("singleton must solve, got {error}"),
                    );
                }
            }
        }
    });
    ph.case("residual-shape-refuses", |ph| {
        let mut result =
            emath_test_harness::Source::from_str("shape-mix", SHAPE_MIX_SRC).must_admit(&mut *ph);
        if result.diagnostics.has_errors() || result.package.declarations.is_empty() {
            return;
        }
        let e1 = result.package.push_expr(
            ExprNode::Literal(Literal::Integer("1".to_string())),
            Span::default(),
        );
        let e2 = result.package.push_expr(
            ExprNode::Literal(Literal::Integer("2".to_string())),
            Span::default(),
        );
        let e3 = result.package.push_expr(
            ExprNode::Literal(Literal::Integer("3".to_string())),
            Span::default(),
        );
        let e4 = result.package.push_expr(
            ExprNode::Literal(Literal::Integer("4".to_string())),
            Span::default(),
        );
        let der_a = result
            .package
            .push_expr(ExprNode::Vector(vec![e1, e2, e3]), Span::default());
        let der_b = result
            .package
            .push_expr(ExprNode::Vector(vec![e4]), Span::default());
        result.package.declarations[0]
            .definitions
            .insert("der_a".to_string(), der_a);
        result.package.declarations[0]
            .definitions
            .insert("der_b".to_string(), der_b);
        let decl = &result.package.declarations[0];
        let mut state = BTreeMap::new();
        state.insert("a".to_string(), Value::Vector(vec![0.0, 0.0]));
        state.insert("b".to_string(), Value::Vector(vec![0.0, 0.0]));
        state.insert("z".to_string(), Value::F64(0.0));
        let outcome = emath_exec_ir::step_continuous_values(
            &result.package,
            decl,
            &BTreeMap::new(),
            &state,
            0.1,
            emath_exec_ir::StepMethod::Euler,
        );
        ph.demand(
            "refuses",
            outcome.is_err(),
            format!("total-width mix must refuse, got {outcome:?}"),
        );
    });
    ph.finish();
}

#[test]
fn runner_contracts() {
    let mut ph = Probe::new("declaration runner verdicts: worked examples compute, expects pass or fail, panes compute or turn symbolic, refusals stay typed");
    runner_square_worked_example_computes(&mut ph);
    runner_square_expect_passes(&mut ph);
    runner_square_expect_fails(&mut ph);
    runner_constant_only_declaration_computes(&mut ph);
    runner_unbound_declaration_returns_symbolic_form_with_label(&mut ph);
    runner_zero_tests_computes_when_inputs_bound(&mut ph);
    runner_definitions_evaluate_in_source_order_not_name_order(&mut ph);
    runner_pane_given_computes_and_empty_binding_is_symbolic(&mut ph);
    runner_named_test_missing_input_still_refuses_typed(&mut ph);
    runner_constructor_refuses_false_require(&mut ph);
    ph.finish();
}
