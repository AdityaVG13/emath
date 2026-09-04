//! Linear programming (slice 1): the LP + multi-objective compute
//! layer.
//!
//! The law, sliced to the numeric-kernel + EMIR seam (the
//! `goal`/`objectives(pareto):` PARSE surface and the MILP
//! branch-and-bound integrality policy are named deferrals — parser
//! lanes and a real design surface respectively):
//! - **LP** (`std.capability.optimize.lp-minimize`): minimize `cᵀx`
//!   s.t. `A x ≤ b`, `x ≥ 0`,
//!   with `b ≥ 0` (the standard-form class — the origin basis is
//!   feasible, so infeasibility cannot arise here; NEGATIVE-right-side
//!   normalization is the named deferral). Deterministic Bland's-rule
//!   simplex: the anti-cycling rule guarantees termination, and every
//!   pivot choice is the smallest index (no hash-order anything).
//!   Unbounded → typed `E-LP-001`, never a wrong finite answer.
//!   Dimension mismatch → `E-LP-003` (the negative seed's shape);
//!   non-finite entries → `E-LP-004` / the registry's all-finite guard.
//! - **Pareto front** (`std.capability.optimize.pareto-front`): rows
//!   of a finite carrier are
//!   objective vectors (ALL MINIMIZED — maximize by negating, the
//!   documented convention). Returns the non-dominated mask in point
//!   index order — the portfolio artifact's deterministic data.
//!   Identical points do not dominate each other (strict Pareto); a
//!   non-finite entry refuses `E-PARETO-001`.
//! - MILP (integer constraints) is the named next slice: it needs the
//!   branch-and-bound node policy, not more vocabulary.

use std::path::{Path, PathBuf};

use emath_core::Span;
use emath_exec_ir::interp::{EvalFault, Value, evaluate_with_budget};
use emath_exec_ir::language_image::load_language_distribution;
use emath_exec_ir::native_kernel::{KernelArity, install_language_distribution, native_kernel};
use emath_exec_ir::{CellClass, EmirOp, EmirProgram, EmirValue, EvalBudget};

fn language_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../language")
}

/// Capability cells resolve against the installed Language Image
/// (thread-local bindings); every evaluating test installs the
/// checked-in distribution first.
fn install_language() {
    let distribution = load_language_distribution(&language_root()).expect("language distribution");
    install_language_distribution(&distribution).expect("optimization kernels install");
}

/// The active universal seam for domain math: an `ApplyCapability`
/// over a capsule-active FeatureID (no domain-named `EmirOp`).
fn cell(capability: &str, args: Vec<EmirValue>) -> EmirOp {
    EmirOp::ApplyCapability {
        capability: capability.to_string(),
        class: CellClass::Pure,
        args,
    }
}

const LP_MINIMIZE: &str = "std.capability.optimize.lp-minimize";
const PARETO_FRONT: &str = "std.capability.optimize.pareto-front";

fn eval(ops: Vec<EmirOp>, inputs: &[Value]) -> Result<Value, EvalFault> {
    install_language();
    // The seam law: LoadInput per input, result = last register.
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
        vec![cell(
            LP_MINIMIZE,
            vec![EmirValue(0), EmirValue(1), EmirValue(2)],
        )],
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
        vec![cell(
            LP_MINIMIZE,
            vec![EmirValue(0), EmirValue(1), EmirValue(2)],
        )],
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
        vec![cell(
            LP_MINIMIZE,
            vec![EmirValue(0), EmirValue(1), EmirValue(2)],
        )],
        &[a, Value::Vector(vec![4.0]), c],
    )
    .expect_err("mismatched b refuses");
    let fault = format!("{error:?}");
    assert!(
        fault.contains("E-LP-003"),
        "dimension mismatch must name E-LP-003, got {fault}"
    );
    const NEGATIVE_SEED: &str =
        include_str!("../../../tests/invalid/linear_program_dimensions.emath");
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
    let mask = eval(vec![cell(PARETO_FRONT, vec![EmirValue(0)])], &[points])
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
    let mask = eval(vec![cell(PARETO_FRONT, vec![EmirValue(0)])], &[points])
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
            1.0,
            f64::NAN, //
            2.0,
            2.0,
        ],
    );
    let error = eval(vec![cell(PARETO_FRONT, vec![EmirValue(0)])], &[points])
        .expect_err("non-finite objective refuses");
    let fault = format!("{error:?}");
    assert!(
        fault.contains("E-PARETO-001"),
        "non-finite objectives must name E-PARETO-001, got {fault}"
    );
}

#[test]
fn linear_program_registry_cell_enforces_shape_law() {
    // std.capability.optimize.lp-minimize / pareto-front: the same
    // solvers as distribution DATA (the anti-LOC law), bound through
    // the checked-in Language Image. The capsule contract's shape law
    // refuses at the kernel ABI: a vector in the constraint-matrix
    // slot refuses typed (E-TYPE-012), never a silently mis-typed
    // solve.
    install_language();
    let lp = native_kernel(LP_MINIMIZE).expect("lp kernel bound");
    assert!(
        matches!(lp.arity_contract(), KernelArity::Exact(3)),
        "lp kernel arity is exact 3"
    );
    assert!(native_kernel(PARETO_FRONT).is_some(), "pareto kernel bound");
    let (a, b, c) = textbook_lp();
    let solution = eval(
        vec![cell(
            LP_MINIMIZE,
            vec![EmirValue(0), EmirValue(1), EmirValue(2)],
        )],
        &[a, b, c],
    )
    .expect("registry lp computes");
    let x = vector_of(&solution);
    let objective = -x[0] - 2.0 * x[1];
    assert!((objective + 6.0).abs() < 1e-7, "registry path: {x:?}");
    // Shape law at the ABI: a vector in A's slot.
    let error = eval(
        vec![cell(
            LP_MINIMIZE,
            vec![EmirValue(0), EmirValue(1), EmirValue(2)],
        )],
        &[
            Value::Vector(vec![4.0, 6.0]),
            Value::Vector(vec![4.0, 6.0]),
            Value::Vector(vec![-1.0, -2.0]),
        ],
    )
    .expect_err("a vector in the constraint-matrix slot refuses");
    let fault = format!("{error:?}");
    assert!(
        fault.contains("E-TYPE-012"),
        "shape law must refuse typed, got {fault}"
    );
}
