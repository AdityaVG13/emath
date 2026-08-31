//! emath-r3-lp-milp-wlif (slice 1): the LP + multi-objective compute
//! layer.
//!
//! The bead's law, sliced to the numeric-kernel + EMIR seam (the
//! `goal`/`objectives(pareto):` PARSE surface and the MILP
//! branch-and-bound integrality policy are named deferrals — parser
//! lanes and a real design surface respectively):
//! - **LP** (`LpMinimize`): minimize `cᵀx` s.t. `A x ≤ b`, `x ≥ 0`,
//!   with `b ≥ 0` (the standard-form class — the origin basis is
//!   feasible, so infeasibility cannot arise here; NEGATIVE-right-side
//!   normalization is the named deferral). Deterministic Bland's-rule
//!   simplex: the anti-cycling rule guarantees termination, and every
//!   pivot choice is the smallest index (no hash-order anything).
//!   Unbounded → typed `E-LP-001`, never a wrong finite answer.
//!   Dimension mismatch → `E-LP-003` (the negative seed's shape);
//!   non-finite entries → `E-LP-004` / the registry's all-finite guard.
//! - **Pareto front** (`ParetoFront`): rows of a finite carrier are
//!   objective vectors (ALL MINIMIZED — maximize by negating, the
//!   documented convention). Returns the non-dominated mask in point
//!   index order — the portfolio artifact's deterministic data.
//!   Identical points do not dominate each other (strict Pareto); a
//!   non-finite entry refuses `E-PARETO-001`.
//! - MILP (integer constraints) is the named next slice: it needs the
//!   branch-and-bound node policy, not more vocabulary.

use emath_core::Span;
use emath_exec_ir::interp::{EvalFault, Value, evaluate_with_budget};
use emath_exec_ir::term_compile::{
    compile_reference, std_cell_registry, ArgGuard, ParamShape, TermCompileError,
};
use emath_exec_ir::{CellClass, EmirOp, EmirProgram, EmirValue, EvalBudget};
use emath_term::{Signature, SymbolId, Term, VariableId};

fn eval(ops: Vec<EmirOp>, inputs: &[Value]) -> Result<Value, EvalFault> {
    // The .14 seam law: LoadInput per input, result = last register.
    let mut program_ops: Vec<(EmirOp, Span)> = (0..inputs.len())
        .map(|index| (EmirOp::LoadInput(index as u16), Span::default()))
        .collect();
    program_ops.extend(ops.into_iter().map(|op| (op, Span::default())));
    let result = EmirValue(program_ops.len() as u32 - 1);
    let program = EmirProgram {
        ops: program_ops,
        result,
        input_count: inputs.len() as u16,
        state_count: 0,
        domain_obligations: Vec::new(),
    };
    evaluate_with_budget(&program, inputs, &[], EvalBudget::default())
}

fn vector_of(value: &Value) -> Vec<f64> {
    let Value::Vector(v) = value else {
        panic!("expected a vector, got {value:?}")
    };
    v.clone()
}

fn matrix(rows: usize, cols: usize, data: &[f64]) -> Value {
    Value::Matrix {
        rows,
        cols,
        data: data.to_vec(),
    }
}

/// Registry-path evaluation of a fixed-shape cell.
fn cell_seval(
    name: &str,
    operator: &str,
    arity: usize,
    params: Vec<(String, ParamShape)>,
    inputs: &[Value],
) -> Result<Value, EvalFault> {
    let term = Term::Apply {
        operator: SymbolId(operator.into()),
        arguments: params
            .iter()
            .map(|(name, _)| Term::Variable(VariableId(name.clone())))
            .collect(),
    };
    let mut signature = Signature::default();
    signature
        .insert(SymbolId(operator.into()), arity)
        .expect("single-operator signature is conflict-free");
    let cell = compile_reference(
        &term,
        &signature,
        &params,
        (0..params.len()).map(ArgGuard::AllFinite).collect(),
        name,
    )
    .expect("cell compiles through the call surface");
    let count = inputs.len();
    let mut ops: Vec<(EmirOp, Span)> = (0..count)
        .map(|index| (EmirOp::LoadInput(index as u16), Span::default()))
        .collect();
    ops.push((
        EmirOp::ApplyCapability {
            capability: name.to_string(),
            class: CellClass::Pure,
            args: (0..count as u32).map(EmirValue).collect(),
        },
        Span::default(),
    ));
    let program = EmirProgram {
        ops,
        result: EmirValue(count as u32),
        input_count: count as u16,
        state_count: 0,
        domain_obligations: Vec::new(),
    };
    evaluate_with_budget(&program, inputs, &[], EvalBudget::default())
}

/// The classic textbook LP: minimize -(x0 + 2 x1) s.t. x0 + x1 ≤ 4,
/// x0 + 2 x1 ≤ 6, x ≥ 0. Optimum objective -6 on the (2,2)/(0,3)
/// edge — alternate optima, so the test pins the OBJECTIVE VALUE and
/// feasibility, not a unique vertex.
fn textbook_lp() -> (Value, Value, Value) {
    (
        matrix(2, 2, &[1.0, 1.0, 1.0, 2.0]),
        Value::Vector(vec![4.0, 6.0]),
        Value::Vector(vec![-1.0, -2.0]),
    )
}

#[test]
fn lp_minimize_returns_known_objective() {
    let (a, b, c) = textbook_lp();
    let solution = eval(
        vec![EmirOp::LpMinimize(EmirValue(0), EmirValue(1), EmirValue(2))],
        &[a, b, c],
    )
    .expect("lp computes");
    let x = vector_of(&solution);
    assert_eq!(x.len(), 2);
    // Objective value law: cᵀx = -6 within tolerance.
    let objective = -x[0] - 2.0 * x[1];
    assert!(
        (objective + 6.0).abs() < 1e-7,
        "optimal objective is -6, got {objective} at {x:?}"
    );
    // Feasibility law: A x ≤ b and x ≥ 0 (the certificate, not trust).
    assert!(x[0] >= -1e-9 && x[1] >= -1e-9, "x ≥ 0, got {x:?}");
    assert!(x[0] + x[1] <= 4.0 + 1e-7, "x0 + x1 ≤ 4, got {x:?}");
    assert!(x[0] + 2.0 * x[1] <= 6.0 + 1e-7, "x0 + 2x1 ≤ 6, got {x:?}");
}

#[test]
fn unbounded_linear_program_refuses_typed() {
    // minimize -x0 s.t. -x0 ≤ 1, x0 ≥ 0: the objective decreases
    // without bound → typed E-LP-001, never a wrong finite "optimum".
    let error = eval(
        vec![EmirOp::LpMinimize(EmirValue(0), EmirValue(1), EmirValue(2))],
        &[
            matrix(1, 1, &[-1.0]),
            Value::Vector(vec![1.0]),
            Value::Vector(vec![-1.0]),
        ],
    )
    .expect_err("unbounded lp refuses");
    let fault = format!("{error:?}");
    assert!(
        fault.contains("E-LP-001"),
        "unbounded lp must name E-LP-001, got {fault}"
    );
}

#[test]
fn linear_program_dimension_mismatch_refuses_typed() {
    // b's length must equal A's row count (E-LP-003) — the negative
    // seed's silent-success shape (a mis-shaped LP must never solve
    // against garbage dimensions).
    let (a, _b, c) = textbook_lp();
    let error = eval(
        vec![EmirOp::LpMinimize(EmirValue(0), EmirValue(1), EmirValue(2))],
        &[a, Value::Vector(vec![4.0]), c],
    )
    .expect_err("mismatched b refuses");
    let fault = format!("{error:?}");
    assert!(
        fault.contains("E-LP-003"),
        "dimension mismatch must name E-LP-003, got {fault}"
    );
    const NEGATIVE_SEED: &str = include_str!("../../../tests/invalid/linear_program_dimensions.emath");
    let expect_line = NEGATIVE_SEED
        .lines()
        .find(|l| l.trim_start().starts_with("# expect:"))
        .expect("seed declares its diagnostic");
    assert!(
        expect_line.contains("E-LP-003"),
        "seed expects the dimension refusal, found: {expect_line}"
    );
}

#[test]
fn pareto_front_computes() {
    // Minimize both coordinates. The non-dominated set of
    // {(4,2),(2,4),(3,3),(1,5),(5,1),(2,2)} is {(1,5),(5,1),(2,2)}:
    // (2,2) dominates (3,3),(4,2),(2,4); the rest are incomparable.
    // The mask is the portfolio artifact's data, in point-index order.
    let points = matrix(
        6,
        2,
        &[
            4.0, 2.0, //
            2.0, 4.0, //
            3.0, 3.0, //
            1.0, 5.0, //
            5.0, 1.0, //
            2.0, 2.0,
        ],
    );
    let mask = eval(
        vec![EmirOp::ParetoFront(EmirValue(0))],
        &[points],
    )
    .expect("pareto front computes");
    assert_eq!(vector_of(&mask), vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0]);
}

#[test]
fn identical_pareto_points_do_not_dominate() {
    // Strict Pareto: identical points do not dominate each other —
    // both stay on the front (the deterministic tie law; a mutant that
    // drops duplicates by index order fails one of the two positions).
    let points = matrix(
        2,
        2,
        &[
            2.0, 2.0, //
            2.0, 2.0,
        ],
    );
    let mask = eval(
        vec![EmirOp::ParetoFront(EmirValue(0))],
        &[points],
    )
    .expect("identical points both survive");
    assert_eq!(vector_of(&mask), vec![1.0, 1.0]);
}

#[test]
fn non_finite_pareto_point_refuses_typed() {
    // A NaN objective entry refuses E-PARETO-001 — never a silently
    // corrupted front (NaN comparisons are always false, which a
    // mutant gate would turn into a wrong mask).
    let points = matrix(
        2,
        2,
        &[
            1.0, f64::NAN, //
            2.0, 2.0,
        ],
    );
    let error = eval(
        vec![EmirOp::ParetoFront(EmirValue(0))],
        &[points],
    )
    .expect_err("non-finite objective refuses");
    let fault = format!("{error:?}");
    assert!(
        fault.contains("E-PARETO-001"),
        "non-finite objectives must name E-PARETO-001, got {fault}"
    );
}

#[test]
fn linear_program_registry_cell_enforces_shape_law() {
    // std.optimize.lp: the same solver as registry DATA (the anti-LOC
    // law), with the all-finite guard declared. A vector in the matrix
    // slot refuses at COMPILE (the closed vocabulary's shape law).
    let registry = std_cell_registry();
    assert!(registry.contains_key("std.optimize.lp"), "lp cell registered");
    assert!(
        registry.contains_key("std.optimize.pareto_front"),
        "pareto cell registered"
    );
    let (a, b, c) = textbook_lp();
    let solution = cell_seval(
        "std.optimize.lp",
        "lp_minimize",
        3,
        vec![
            ("A".to_string(), ParamShape::Matrix),
            ("b".to_string(), ParamShape::Vector),
            ("c".to_string(), ParamShape::Vector),
        ],
        &[a, b, c],
    )
    .expect("registry lp computes");
    let x = vector_of(&solution);
    let objective = -x[0] - 2.0 * x[1];
    assert!((objective + 6.0).abs() < 1e-7, "registry path: {x:?}");
    // Shape law at compile: a vector in A's slot.
    let term = Term::Apply {
        operator: SymbolId("lp_minimize".into()),
        arguments: vec![
            Term::Variable(VariableId("A".into())),
            Term::Variable(VariableId("b".into())),
            Term::Variable(VariableId("c".into())),
        ],
    };
    let mut signature = Signature::default();
    signature
        .insert(SymbolId("lp_minimize".into()), 3)
        .expect("signature is conflict-free");
    let error = compile_reference(
        &term,
        &signature,
        &[
            ("A".to_string(), ParamShape::Vector),
            ("b".to_string(), ParamShape::Vector),
            ("c".to_string(), ParamShape::Vector),
        ],
        Vec::new(),
        "surface.shape-law-lp",
    )
    .expect_err("a vector in the constraint-matrix slot refuses at compile");
    assert!(
        matches!(error, TermCompileError::ShapeMismatch { .. }),
        "shape law must refuse, got {error:?}"
    );
}
