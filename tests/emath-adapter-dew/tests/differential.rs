//! Phase 2 native exec-ir ↔ Dew-adapter differential over the scalar corpus.
//!
//! Both paths are strict-f64 IEEE-754 binary64. Agreement is bit-exact
//! (`to_bits`), including signed zero. Transcendentals are not in this
//! corpus, so no libm tolerance is required.

use std::collections::BTreeMap;

use emath_adapter_dew::{EvalValue, evaluate_scalar, map_expression};
use emath_core::{QualifiedName, Span};
use emath_exec_ir::interp::{Value, evaluate};
use emath_exec_ir::lower_definition;
use emath_ir::{BinaryOp, ExprId, ExprNode, SemanticPackage};

const SQUARE_SRC: &str = include_str!("../../valid/square.emath");
const AFFINE_SRC: &str = include_str!("../../valid/affine_scorer.emath");

struct CorpusCase {
    name: &'static str,
    package: SemanticPackage,
    expr: ExprId,
    inputs: Vec<String>,
    states: Vec<String>,
    input_values: Vec<f64>,
    state_values: Vec<f64>,
}

fn var(package: &mut SemanticPackage, name: &str) -> ExprId {
    package.push_expr(
        ExprNode::Variable(QualifiedName::single(name)),
        Span::default(),
    )
}

fn binary(
    package: &mut SemanticPackage,
    operation: BinaryOp,
    left: ExprId,
    right: ExprId,
) -> ExprId {
    package.push_expr(
        ExprNode::Binary {
            operation,
            left,
            right,
        },
        Span::default(),
    )
}

/// `y = x * x` from `tests/valid/square.emath`.
fn square_case(name: &'static str, x: f64) -> CorpusCase {
    let mut package = SemanticPackage::new();
    let x_var = var(&mut package, "x");
    let expr = binary(&mut package, BinaryOp::StrictFloatMul, x_var, x_var);
    CorpusCase {
        name,
        package,
        expr,
        inputs: vec!["x".into()],
        states: Vec::new(),
        input_values: vec![x],
        state_values: Vec::new(),
    }
}

/// `score = state.scale * x + state.bias` from `tests/valid/affine_scorer.emath`.
fn affine_case(name: &'static str, x: f64, scale: f64, bias: f64) -> CorpusCase {
    let mut package = SemanticPackage::new();
    let x_var = var(&mut package, "x");
    let scale_var = var(&mut package, "state.scale");
    let bias_var = var(&mut package, "state.bias");
    let prod = binary(&mut package, BinaryOp::StrictFloatMul, scale_var, x_var);
    let expr = binary(&mut package, BinaryOp::StrictFloatAdd, prod, bias_var);
    CorpusCase {
        name,
        package,
        expr,
        inputs: vec!["x".into()],
        states: vec!["scale".into(), "bias".into()],
        input_values: vec![x],
        state_values: vec![scale, bias],
    }
}

/// Adapter fixture `x + 1` (existing oracle `scalar_expr`).
fn plus_one_case(name: &'static str, x: f64) -> CorpusCase {
    let mut package = SemanticPackage::new();
    let x_var = var(&mut package, "x");
    let one = package.push_expr(
        ExprNode::Literal(emath_ir::Literal::FloatBits(1.0f64.to_bits())),
        Span::default(),
    );
    let expr = binary(&mut package, BinaryOp::StrictFloatAdd, x_var, one);
    CorpusCase {
        name,
        package,
        expr,
        inputs: vec!["x".into()],
        states: Vec::new(),
        input_values: vec![x],
        state_values: Vec::new(),
    }
}

fn scalar_corpus() -> Vec<CorpusCase> {
    let mut cases = Vec::new();

    // Official `tests/valid/square.emath` example plus a seeded x-grid.
    cases.push(square_case("square/three_squared", 3.0));
    for x in [-2.0, -1.0, -0.0, 0.0, 0.5, 1.0, 1.5, 2.0, 4.0] {
        cases.push(square_case("square/grid", x));
    }

    // Official `tests/valid/affine_scorer.emath` examples plus a seeded grid.
    cases.push(affine_case("affine/score_is_seven", 3.0, 1.0, 4.0));
    cases.push(affine_case("affine/fractional_score", 1.5, 2.0, 0.5));
    for x in [0.0, 1.0, 2.0] {
        for scale in [0.5, 1.0, 2.0] {
            for bias in [0.0, -1.0] {
                cases.push(affine_case("affine/grid", x, scale, bias));
            }
        }
    }

    // Existing Dew oracle fixture `x + 1`.
    for x in [-2.0, -0.0, 0.0, 0.5, 1.0, 2.0] {
        cases.push(plus_one_case("fixture/x+1", x));
    }

    cases
}

fn dew_env(case: &CorpusCase) -> BTreeMap<String, f64> {
    let mut env = BTreeMap::new();
    for (name, value) in case.inputs.iter().zip(case.input_values.iter()) {
        env.insert(name.clone(), *value);
    }
    for (name, value) in case.states.iter().zip(case.state_values.iter()) {
        env.insert(format!("state.{name}"), *value);
    }
    env
}

fn native_bits(case: &CorpusCase) -> u64 {
    let program = lower_definition(&case.package, case.expr, &case.inputs, &case.states)
        .unwrap_or_else(|error| panic!("{}: native lower refused: {error}", case.name));
    match evaluate(&program, &case.input_values, &case.state_values)
        .unwrap_or_else(|fault| panic!("{}: native eval fault: {fault}", case.name))
    {
        Value::F64(value) => value.to_bits(),
        Value::Bool(value) => u64::from(value),
    }
}

fn dew_bits(case: &CorpusCase) -> u64 {
    let dew = map_expression(&case.package, case.expr)
        .unwrap_or_else(|issue| panic!("{}: Dew map refused: {}", case.name, issue.detail));
    match evaluate_scalar(&dew, &dew_env(case)) {
        Some(EvalValue::F64(value)) => value.to_bits(),
        Some(EvalValue::Bool(value)) => u64::from(value),
        None => panic!("{}: Dew evaluator undefined", case.name),
    }
}

#[test]
fn native_and_dew_agree_on_scalar_corpus() {
    assert!(
        SQUARE_SRC.contains("y = x * x"),
        "corpus must stay tied to tests/valid/square.emath"
    );
    assert!(
        AFFINE_SRC.contains("score = state.scale * x + state.bias"),
        "corpus must stay tied to tests/valid/affine_scorer.emath"
    );

    let cases = scalar_corpus();
    assert_eq!(
        cases.len(),
        36,
        "corpus size is part of the evidence claim (10 square + 20 affine + 6 fixture)"
    );

    for case in &cases {
        let native = native_bits(case);
        let dew = dew_bits(case);
        assert_eq!(
            native, dew,
            "{}: native bits {native:016x} != Dew bits {dew:016x}",
            case.name
        );
    }
}
