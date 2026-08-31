//! Declaration-runner tests migrated from the in-crate `#[cfg(test)]`
//! module: the runner entry points (`run_package`, `run_package_with_given`)
//! and the `SemanticPackage` builder are public crate surface, so these
//! exercise the API exactly as an embedder would.

use std::collections::BTreeMap;

use emath_core::{QualifiedName, Span};
use emath_exec_ir::interp::Value;
use emath_exec_ir::runner::{
    run_package, run_package_with_given, PANE_TEST_NAME, TestVerdict, ZERO_TEST_NOTE,
};
use emath_ir::{
    BinaryOp, Constructor, DeclarationId, ExprNode, Field, Literal, SemanticPackage, TypeNode,
    Visibility,
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
