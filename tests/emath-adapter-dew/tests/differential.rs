//! Phase 2 native exec-ir ↔ Dew-adapter differential over the scalar corpus.
use std::collections::{BTreeMap, BTreeSet};
use emath_adapter_dew::{EvalValue, evaluate_scalar, map_expression};
use emath_core::{QualifiedName, Span};
use emath_exec_ir::interp::{Value, evaluate};
use emath_exec_ir::lower_definition;
use emath_ir::{BinaryOp, ExprId, ExprNode, SemanticPackage};
use emath_lab_core::{EngineIdentity, EngineRole};
use emath_test_harness::Probe;

const SQUARE_SRC: &str = include_str!("../../valid/square.emath");
const AFFINE_SRC: &str = include_str!("../../valid/affine_scorer.emath");

struct CorpusCase {
    name: String,
    package: SemanticPackage,
    expr: ExprId,
    inputs: Vec<String>,
    states: Vec<String>,
    input_values: Vec<f64>,
    state_values: Vec<f64>,
}
fn var(package: &mut SemanticPackage, name: &str) -> ExprId {
    package.push_expr(ExprNode::Variable(QualifiedName::single(name)), Span::default())
}
fn binary(package: &mut SemanticPackage, op: BinaryOp, l: ExprId, r: ExprId) -> ExprId {
    package.push_expr(ExprNode::Binary { operation: op, left: l, right: r }, Span::default())
}
fn square_case(name: String, x: f64) -> CorpusCase {
    let mut package = SemanticPackage::new();
    let x_var = var(&mut package, "x");
    let expr = binary(&mut package, BinaryOp::StrictFloatMul, x_var, x_var);
    CorpusCase { name, package, expr, inputs: vec!["x".into()], states: vec![], input_values: vec![x], state_values: vec![] }
}
fn affine_case(name: String, x: f64, scale: f64, bias: f64) -> CorpusCase {
    let mut package = SemanticPackage::new();
    let (x_var, s_var, b_var) = (var(&mut package, "x"), var(&mut package, "state.scale"), var(&mut package, "state.bias"));
    let prod = binary(&mut package, BinaryOp::StrictFloatMul, s_var, x_var);
    let expr = binary(&mut package, BinaryOp::StrictFloatAdd, prod, b_var);
    CorpusCase { name, package, expr, inputs: vec!["x".into()], states: vec!["scale".into(), "bias".into()], input_values: vec![x], state_values: vec![scale, bias] }
}
fn plus_one_case(name: String, x: f64) -> CorpusCase {
    let mut package = SemanticPackage::new();
    let x_var = var(&mut package, "x");
    let one = package.push_expr(ExprNode::Literal(emath_ir::Literal::FloatBits(1.0f64.to_bits())), Span::default());
    let expr = binary(&mut package, BinaryOp::StrictFloatAdd, x_var, one);
    CorpusCase { name, package, expr, inputs: vec!["x".into()], states: vec![], input_values: vec![x], state_values: vec![] }
}
fn scalar_corpus() -> Vec<CorpusCase> {
    let mut cases = vec![square_case("square/three_squared".into(), 3.0)];
    for x in [-2.0, -1.0, -0.0, 0.0, 0.5, 1.0, 1.5, 2.0, 4.0] {
        cases.push(square_case(format!("square/grid/x={x}"), x));
    }
    cases.push(affine_case("affine/score_is_seven".into(), 3.0, 1.0, 4.0));
    cases.push(affine_case("affine/fractional_score".into(), 1.5, 2.0, 0.5));
    for x in [0.0, 1.0, 2.0] {
        for scale in [0.5, 1.0, 2.0] {
            for bias in [0.0, -1.0] {
                cases.push(affine_case(format!("affine/grid/x={x}/scale={scale}/bias={bias}"), x, scale, bias));
            }
        }
    }
    for x in [-2.0, -0.0, 0.0, 0.5, 1.0, 2.0] {
        cases.push(plus_one_case(format!("fixture/x+1/x={x}"), x));
    }
    cases
}
fn dew_env(case: &CorpusCase) -> BTreeMap<String, f64> {
    let mut env = BTreeMap::new();
    for (n, v) in case.inputs.iter().zip(case.input_values.iter()) {
        env.insert(n.clone(), *v);
    }
    for (n, v) in case.states.iter().zip(case.state_values.iter()) {
        env.insert(format!("state.{n}"), *v);
    }
    env
}
fn native_bits(case: &CorpusCase) -> u64 {
    let program = lower_definition(&case.package, case.expr, &case.inputs, &case.states)
        .unwrap_or_else(|e| panic!("{}: native lower refused: {e}", case.name));
    let ins: Vec<Value> = case.input_values.iter().map(|&v| Value::F64(v)).collect();
    let sts: Vec<Value> = case.state_values.iter().map(|&v| Value::F64(v)).collect();
    match evaluate(&program, &ins, &sts).unwrap_or_else(|f| panic!("{}: native fault: {f}", case.name)) {
        Value::F64(v) => v.to_bits(),
        Value::Bool(v) => u64::from(v),
        _ => panic!("{}: non-scalar native value", case.name),
    }
}
fn dew_bits(case: &CorpusCase) -> u64 {
    let dew = map_expression(&case.package, case.expr).unwrap_or_else(|i| panic!("{}: dew map refused: {}", case.name, i.detail));
    match evaluate_scalar(&dew, &dew_env(case)) {
        Some(EvalValue::F64(v)) => v.to_bits(),
        Some(EvalValue::Bool(v)) => u64::from(v),
        None => panic!("{}: dew undefined", case.name),
    }
}

#[test]
fn probe() {
    let mut p = Probe::new("native exec-ir and dew adapter agree bit-exact over the scalar corpus");
    p.contains("square-tie", SQUARE_SRC, "y = x * x");
    p.contains("affine-tie", AFFINE_SRC, "score = state.scale * x + state.bias");
    let (subject, oracle) = (
        EngineIdentity { role: EngineRole::Subject, label: "emath-exec-ir-native".into() },
        EngineIdentity { role: EngineRole::Oracle, label: "emath-adapter-dew".into() },
    );
    p.demand("distinct-engines", subject.require_distinct(&oracle, "dew differential").is_ok(), "engines must be distinct");
    let cases = scalar_corpus();
    p.eq("corpus-size", cases.len(), 36);
    let mut ids = BTreeSet::new();
    p.case("unique-ids", |p| {
        for c in &cases {
            p.demand(c.name.clone(), ids.insert(c.name.clone()), format!("duplicate id {}", c.name));
        }
    });
    p.case("bit-agreement", |p| {
        for c in &cases {
            p.eq(c.name.clone(), native_bits(c), dew_bits(c));
        }
    });
    p.finish();
}
