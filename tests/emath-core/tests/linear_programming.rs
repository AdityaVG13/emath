//! `emath-r3-lp-milp-wlif`: LP/MILP solver nucleus + Pareto front
//! (B24 + B36) — contract tests.
//!
//! The solver is the finite-carrier machinery the bead's surface rides
//! on: a deterministic two-phase simplex (Bland's rule, no cycling)
//! for LP over f64, branch-and-bound for MILP with declared integer
//! tolerance, and a deterministic Pareto-front (nondominated set)
//! helper for the portfolio pattern. The `.emath` goal surface
//! (`objectives(pareto):` sections, goal-kind admission) is the
//! documented follow-up gated on the sema hold — this file pins the
//! SOLVER contract that surface will lower into.
//!
//! Failure-first: RED until `emath_core::linprog` lands.

use emath_core::linprog::{
    pareto_front, Constraint, LinProg, MilpSolution, Sense, Solution,
};

#[test]
fn lp_canonical_problem_solves() {
    // Oracle correction (documented): the first spelling (max 3x+2y
    // with 3x+2y ≤ 18) has a DEGENERATE optimal face — (2,6) and (4,3)
    // both reach 18 — so pinning one vertex over-constrains the
    // solver. The corrected program tilts the objective (max 4x+3y),
    // making the optimum unique at the same intended vertex (2,6):
    // 4·2+3·6 = 26.
    let program = LinProg::minimize(&[-4.0, -3.0])
        .constraint(Constraint::le(&[1.0, 0.0], 4.0))
        .constraint(Constraint::le(&[0.0, 1.0], 6.0))
        .constraint(Constraint::le(&[3.0, 2.0], 18.0));
    let Solution::Optimal { primal, objective } = program.solve() else {
        panic!("canonical LP must solve, got {:?}", program.solve());
    };
    assert!((objective - (-26.0)).abs() < 1e-9, "objective was {objective}");
    assert!((primal[0] - 2.0).abs() < 1e-9 && (primal[1] - 6.0).abs() < 1e-9);
}

#[test]
fn lp_equality_constraint_solves() {
    // min x + y  s.t. x + y = 5, x ≥ 1, y ≥ 1 → 5 (any split; the
    // objective only pins the sum).
    let program = LinProg::minimize(&[1.0, 1.0])
        .constraint(Constraint::eq(&[1.0, 1.0], 5.0))
        .constraint(Constraint::ge(&[1.0, 0.0], 1.0))
        .constraint(Constraint::ge(&[0.0, 1.0], 1.0));
    let Solution::Optimal { objective, primal } = program.solve() else {
        panic!("equality LP must solve");
    };
    assert!((objective - 5.0).abs() < 1e-9);
    assert!((primal[0] + primal[1] - 5.0).abs() < 1e-9);
    assert!(primal[0] >= 1.0 - 1e-9 && primal[1] >= 1.0 - 1e-9);
}

#[test]
fn infeasible_lp_refuses_named() {
    // x ≥ 3 and x ≤ 2: an empty feasible set is a named status, never
    // a garbage optimum.
    let program = LinProg::minimize(&[1.0])
        .constraint(Constraint::ge(&[1.0], 3.0))
        .constraint(Constraint::le(&[1.0], 2.0));
    assert!(matches!(program.solve(), Solution::Infeasible));
}

#[test]
fn unbounded_lp_refuses_named() {
    // min −x with no upper bound on x.
    let program = LinProg::minimize(&[-1.0]);
    assert!(matches!(program.solve(), Solution::Unbounded));
}

#[test]
fn milp_integer_constraint_solves() {
    // max x + y  s.t. x + y ≤ 10.5, x, y ≥ 0 integers → 10 (e.g. (5, 5)).
    let program = LinProg::maximize(&[1.0, 1.0])
        .constraint(Constraint::le(&[1.0, 1.0], 10.5))
        .with_integrality(vec![true, true]);
    let MilpSolution::Optimal { primal, objective } = program.solve_milp(1e-9) else {
        panic!("MILP must solve, got {:?}", program.solve_milp(1e-9));
    };
    assert!((objective - 10.0).abs() < 1e-9, "objective was {objective}");
    assert!(
        primal[0].fract().abs() < 1e-9 && primal[1].fract().abs() < 1e-9,
        "integer solution must be integral: {primal:?}"
    );
}

#[test]
fn milp_rounding_trap_is_not_taken() {
    // The trap: LP relaxation optimum is (2.5, 2.5) with x + y ≤ 5,
    // x ≤ 2.5, y ≤ 3.5 — per-variable rounding gives (3,3) (infeasible:
    // violates both the sum and the x cap) or (2,2) (suboptimal, 4).
    // The integer optimum (2,3) = 5 requires BRANCHING. (Oracle
    // correction, documented: the first spelling capped y at 2.5 too,
    // which makes 4 the true optimum — the trap did not exist there.)
    let program = LinProg::maximize(&[1.0, 1.0])
        .constraint(Constraint::le(&[1.0, 1.0], 5.0))
        .constraint(Constraint::le(&[1.0, 0.0], 2.5))
        .constraint(Constraint::le(&[0.0, 1.0], 3.5))
        .with_integrality(vec![true, true]);
    let MilpSolution::Optimal { objective, primal } = program.solve_milp(1e-9) else {
        panic!("B&B must solve the rounding-trap MILP");
    };
    assert!((objective - 5.0).abs() < 1e-9, "objective was {objective}");
    assert!(
        (primal[0] - 2.0).abs() < 1e-9 && (primal[1] - 3.0).abs() < 1e-9,
        "primal was {primal:?}"
    );
}

#[test]
fn milp_infeasible_integers_refuse() {
    // x = 0.5 forced integral: no integer point exists.
    let program = LinProg::minimize(&[1.0])
        .constraint(Constraint::eq(&[1.0], 0.5))
        .with_integrality(vec![true]);
    assert!(matches!(program.solve_milp(1e-9), MilpSolution::Infeasible));
}

#[test]
fn solver_is_deterministic() {
    // Same program, same solution — Bland's rule forbids pivot cycling
    // and the branch order is fixed (lowest-index fractional, best
    // objective first). Two runs agree bit-for-bit on the trace shape.
    let build = || {
        LinProg::minimize(&[-1.0, -1.0, -1.0])
            .constraint(Constraint::le(&[1.0, 2.0, 3.0], 6.0))
            .constraint(Constraint::le(&[2.0, 1.0, 1.0], 4.0))
            .with_integrality(vec![false, true, false])
    };
    let a = build().solve_milp(1e-9);
    let b = build().solve_milp(1e-9);
    assert_eq!(format!("{a:?}"), format!("{b:?}"));
}

#[test]
fn pareto_front_nondominated_set() {
    // Oracle correction (documented): under MINIMIZATION dominance,
    // (2,2) dominates BOTH (2,3) and (3,2) — the first draft of this
    // pin had the direction backwards. Front of
    // {(1,4), (2,3), (3,2), (2,2), (4,1)} = {(1,4), (2,2), (4,1)}.
    let front = pareto_front(&[(1.0, 4.0), (2.0, 3.0), (3.0, 2.0), (2.0, 2.0), (4.0, 1.0)]);
    assert_eq!(front, vec![(1.0, 4.0), (2.0, 2.0), (4.0, 1.0)]);
}

#[test]
fn pareto_front_handles_duplicates_and_order() {
    // Duplicates are mutually nondominated (kept once, stable);
    // input order must not change the front's membership. Here
    // (2,2) appears twice, (1,1) dominates it — wait, it does:
    // 1 ≤ 2 and 1 ≤ 2 with strictness — so (2,2) is dominated and
    // leaves the front entirely; the duplicate-collapse case is
    // covered by the (2,2) pair itself being checked.
    let front = pareto_front(&[(2.0, 2.0), (2.0, 2.0), (1.0, 3.0), (3.0, 1.0)]);
    assert_eq!(front, vec![(2.0, 2.0), (1.0, 3.0), (3.0, 1.0)]);
    let dominated = pareto_front(&[(2.0, 2.0), (1.0, 1.0), (3.0, 3.0)]);
    assert_eq!(dominated, vec![(1.0, 1.0)]);
}
