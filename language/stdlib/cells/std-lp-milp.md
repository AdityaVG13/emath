# `core::lp_milp`; solver nucleus contract (B24 + B36, Phase 6/7 surface)

Status: **solver nucleus landed** (bead `emath-r3-lp-milp-wlif`): deterministic LP
simplex, branch-and-bound MILP, and the Pareto-front helper in
`crates/emath-core/src/linprog.rs`. The `.emath` goal surface (`objectives(pareto):`
section heads, goal-kind admission, `solve_linear` vs root-`solve` distinction) is the
documented follow-up; it lowers INTO this contract and is gated on the sema admission
table (held lane at landing time).

## Carrier and determinism contract

- Variables are NONNEGATIVE (`x ≥ 0` is the declared standard-form carrier; free and
  general-bounded variables are follow-ups).
- LP: two-phase primal simplex with **Bland's rule** (lowest-index entering column;
  ratio ties break to the lowest basis column). Pivot cycling is impossible and the
  pivot sequence is a pure function of the program; two runs agree exactly.
- MILP: branch-and-bound, depth-first with the **floor branch explored first**,
  branching always on the **lowest-index fractional integer variable**; the declared
  `integer_tol` separates integral from fractional. A node budget exists: exhaustion
  reports `NodeLimit` with the best known point; never a false optimal claim.
- Statuses are named: `Infeasible` and `Unbounded` are answers, not garbage optima.

## API

- `LinProg::minimize(c)` / `maximize(c)` + `.constraint(Constraint::le|ge|eq(..))`
  + `.with_integrality(vec![bool; n])`.
- `solve() -> Solution`; LP relaxation: `Optimal { primal, objective }` (objective in
  the DECLARED sense), `Infeasible`, `Unbounded`.
- `solve_milp(integer_tol) -> MilpSolution`; B&B: `Optimal`, `Infeasible`,
  `Unbounded`, `NodeLimit { primal, objective }`.
- `pareto_front(&[(f64, f64)]) -> Vec<(f64, f64)>`; nondominated set over 2-objective
  MINIMIZATION; duplicates collapse (mutually nondominated); stable first-occurrence
  order.

## No-claim boundaries

- Exact rational arithmetic is NOT claimed: the simplex runs on f64 with the declared
  `1e-9` / `1e-12` tolerances; degenerate problems with multiple optima return ONE
  vertex (deterministically chosen), not a canonical representative of the face.
- The Pareto helper is 2-objective minimization; k-objective fronts and maximization
  axes are follow-ups (the portfolio crate's record-domain Pareto selection is the
  artifact-layer seam).
- The `.emath` surface does not admit these goals yet (see Status); e2e model
  compilation is the follow-up.
