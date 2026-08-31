# `core::optimization` methods nucleus — continuous advanced methods (xx0x.6)

Status: **Newton + BFGS engines landed** (bead `emath-xx0x.6`, emath-core
reference nucleus). The admitted `minimize`/`maximize` goal surface
(pure Newton + quadratic penalty, exec-ir lane) is unchanged; goal-
surface METHOD SELECTION over this nucleus is the declared wiring
follow-up (exec-ir foreign hold at landing time).

## Contract

| Engine | Inputs | Step | Convergence | Refusals |
|---|---|---|---|---|
| `newton` | f, ∇f, H (analytic or AD closures) | `H d = −∇f`, Armijo backtracking line search | `‖∇f‖_∞ ≤ tolerance` | `SingularHessian` (no pseudo-inverse), `BudgetExhausted{iterations, ‖∇f‖}` (achieved gradient travels with the refusal), `LineSearchStalled` (no Armijo decrease — e.g. an ascent direction on negative curvature) |
| `bfgs` | f, ∇f only | `d = −H∇f` with the inverse-Hessian approximation (identity start), rank-2 update when `yᵀs > 1e-10` | same | same + the curvature-condition skip keeps H positive definite instead of corrupting it |
| `kkt_residual` | ∇f, {∇cᵢ}, {λᵢ}, {cᵢ} | `‖(∇f + Σλᵢ∇cᵢ, c)‖_∞` | 0 only at a KKT point | — (certificate helper) |
| `interior_point` | — | — | — | **refuses by name** (log-barrier carrier is the follow-up); a constrained problem never silently falls back to the unconstrained engine |
| `sqp` | — | — | — | **refuses by name** (QP-subproblem carrier is the follow-up) |

`SolverOptions` declares the stationarity tolerance and the explicit
iteration budget (no unbounded solve). `SolverOutcome` carries x, f(x),
the achieved `‖∇f‖_∞`, and the iterations spent.

## Honesty boundaries

- **Stationary, not global**: these engines find points where
  `∇f ≈ 0`. Global/Bayesian/manifold/bilevel optimization stays on the
  domain epic and refuses here. Rosenbrock's (1,1) is claimed as the
  reached stationary point, not as "the global minimum proven".
- Convergence is GRADIENT-based by declaration: a flat basin must not
  read as converged while `‖∇f‖` is large.
- The admitted goal surface today (`minimize`/`maximize … wrt`) is
  pure Newton + quadratic penalty (see `optimize.emath` /
  `constrained-opt.emath`): the inequality there is approached, not a
  hard feasible projection — that honesty label carries over until the
  constrained methods land.
- Reverse-mode AD (xx0x.1 `grad()`, exec-ir lane) is the production
  gradient provider; the nucleus takes analytic or AD closures through
  the same closure seam. Finite-difference Hessians (the exec-ir path)
  are labeled FD wherever used.

## Implementation

`crates/emath-core/src/optimization.rs` — std-only. Dense solves by
Gaussian elimination with partial pivoting and a relative singularity
threshold (`SingularHessian` below it). Armijo constants `c = 1e-4`,
50 halvings. No allocation beyond the working vectors; deterministic;
no randomness anywhere.

## No-claim boundaries

- No SDP/SOCP claim (domain-phased, per the bead). No MPC (dynamics
  domain). No MILP here (B24's `core::lp_milp` owns discrete; the two
  cells cross-link instead of overlapping).
- Certificates/duality depth belongs with the domain epic + LP bead:
  `kkt_residual` is a residual, not a duality certificate.
- Mutation-disclosed envelope note: for the PSD systems this nucleus
  reaches productively, partial pivoting is a standard-choice
  equivalent (no-pivot elimination has bounded growth on PSD); the
  pivot code stays as the numerically standard choice for the
  indefinite systems the constrained methods will bring.
