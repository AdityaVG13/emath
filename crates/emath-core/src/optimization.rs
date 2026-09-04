//! Optimizer nucleus. std-only.
//!
//! Engines: **Newton** (analytic Hessian + Armijo backtracking line
//! search) and **BFGS** (quasi-Newton — objective + gradient only),
//! plus the KKT residual helper for the constrained story. Interior-
//! point and SQP refuse by name (the constrained
//! methods arrive with the goal-surface constraints work, never as a
//! silent fallback of the unconstrained engine).
//!
//! Honesty contract:
//! - These methods find STATIONARY points — a LOCAL claim. Global
//!   optimality is never claimed (the domain epic owns global /
//!   Bayesian / manifold / bilevel, which refuse here).
//! - Convergence is declared as `‖∇f‖_∞ ≤ tolerance` (stationarity),
//!   never objective-change or step-size: a flat basin must not read
//!   as converged while the gradient is large.
//! - A singular Hessian refuses BY NAME (no pseudo-inverse solve); an
//!   exhausted budget refuses carrying the achieved `‖∇f‖_∞` — a
//!   non-solution is never dressed as a result.
//! - Reverse-mode AD (the `grad()` builtin) is the
//!   production gradient provider; this nucleus takes analytic or AD
//!   closures through the same function-pointer seam, so the
//!   goal-surface wiring (method selection per goal —
//!   follow-up) reuses these engines
//!   without re-deriving them.

/// Declared solver configuration: stationarity tolerance and the
/// explicit iteration budget (an optimization must never run
/// unbounded).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SolverOptions {
    /// Converged when `‖∇f‖_∞ ≤ tolerance`.
    pub tolerance: f64,
    /// Maximum Newton/BFGS iterations; exhaustion is a refusal that
    /// carries the achieved gradient norm.
    pub max_iterations: usize,
}

impl Default for SolverOptions {
    fn default() -> Self {
        SolverOptions {
            tolerance: 1e-8,
            max_iterations: 200,
        }
    }
}

/// A converged stationary point: the location, the objective value
/// there, the achieved gradient norm, and the iterations spent.
#[derive(Debug, Clone, PartialEq)]
pub struct SolverOutcome {
    pub x: Vec<f64>,
    pub objective: f64,
    pub gradient_inf_norm: f64,
    pub iterations: usize,
}

/// Typed refusals — every non-solution is named, never dressed up.
#[derive(Debug, Clone, PartialEq)]
pub enum OptimizeFault {
    /// The Hessian (or BFGS direction system) was numerically singular:
    /// the step is undefined and a pseudo-inverse would be a lie.
    SingularHessian { iteration: usize },
    /// The iteration budget ran out before `‖∇f‖_∞ ≤ tolerance`. The
    /// achieved gradient norm travels with the refusal as evidence.
    BudgetExhausted {
        iterations: usize,
        gradient_inf_norm: f64,
    },
    /// Backtracking could not find a decrease along the direction:
    /// the direction was not a descent direction (numerical breakdown,
    /// e.g. a lost positive-definiteness) — refused, not stepped blind.
    LineSearchStalled { iteration: usize },
}

/// Armijo backtracking line search along `direction` from `x`:
/// returns the accepted step length `t` with
/// `f(x + t·d) ≤ f(x) + c·t·∇fᵀd`, or `None` when 50 halvings find no
/// decrease (the caller refuses — see [`OptimizeFault::LineSearchStalled`]).
fn line_search(
    f: &dyn Fn(&[f64]) -> f64,
    x: &[f64],
    slope: f64,
    direction: &[f64],
) -> Option<(f64, f64)> {
    const C: f64 = 1e-4;
    const HALVINGS: usize = 50;
    let base = f(x);
    let mut t = 1.0_f64;
    for _ in 0..HALVINGS {
        let step: Vec<f64> = x
            .iter()
            .zip(direction.iter())
            .map(|(&xi, &di)| xi + t * di)
            .collect();
        let value = f(&step);
        if value.is_finite() && value <= base + C * t * slope {
            return Some((t, value));
        }
        t *= 0.5;
    }
    None
}

/// Dense linear solve `A d = b` by Gaussian elimination with partial
/// pivoting. A pivot below `1e-12` (relative to the column norm) means
/// a numerically singular system: `None`, not a pseudo-solution.
fn solve_dense(a: &[Vec<f64>], b: &[f64]) -> Option<Vec<f64>> {
    let n = b.len();
    let mut m = a.to_vec();
    for (row, rhs) in m.iter_mut().zip(b.iter()) {
        row.push(*rhs);
    }
    for col in 0..n {
        let pivot = (col..n)
            .map(|row| (row, m[row][col].abs()))
            .max_by(|x, y| x.1.total_cmp(&y.1))?;
        if pivot.1 < 1e-12 {
            return None;
        }
        m.swap(col, pivot.0);
        let inv = 1.0 / m[col][col];
        for row in (col + 1)..n {
            let factor = m[row][col] * inv;
            if factor != 0.0 {
                for k in col..=n {
                    m[row][k] -= factor * m[col][k];
                }
            }
        }
    }
    let mut out = vec![0.0_f64; n];
    for row in (0..n).rev() {
        let mut acc = m[row][n];
        for k in (row + 1)..n {
            acc -= m[row][k] * out[k];
        }
        out[row] = acc / m[row][row];
    }
    Some(out)
}

fn gradient_inf_norm(grad: &[f64]) -> f64 {
    grad.iter().fold(0.0_f64, |acc, g| acc.max(g.abs()))
}

/// Newton's method for `min f(x)` with an analytic (or AD-supplied)
/// gradient and Hessian: iterate `x += t·d` with `H d = −∇f`, Armijo
/// line search choosing `t`. Converged = `‖∇f‖_∞ ≤ tolerance`.
/// Singular Hessian and exhausted budget refuse (typed).
pub fn newton(
    f: &dyn Fn(&[f64]) -> f64,
    grad: &dyn Fn(&[f64]) -> Vec<f64>,
    hess: &dyn Fn(&[f64]) -> Vec<Vec<f64>>,
    x0: &[f64],
    options: &SolverOptions,
) -> Result<SolverOutcome, OptimizeFault> {
    let mut x = x0.to_vec();
    for iteration in 0..options.max_iterations {
        let g = grad(&x);
        let norm = gradient_inf_norm(&g);
        if norm <= options.tolerance {
            return Ok(SolverOutcome {
                gradient_inf_norm: norm,
                objective: f(&x),
                x,
                iterations: iteration,
            });
        }
        let h = hess(&x);
        let direction = solve_dense(&h, &g.iter().map(|v| -v).collect::<Vec<_>>())
            .ok_or(OptimizeFault::SingularHessian { iteration })?;
        let slope: f64 = g.iter().zip(direction.iter()).map(|(gi, di)| gi * di).sum();
        let Some((_t, _value)) = line_search(f, &x, slope, &direction) else {
            return Err(OptimizeFault::LineSearchStalled { iteration });
        };
        for (xi, di) in x.iter_mut().zip(direction.iter()) {
            *xi += _t * di;
        }
    }
    let g = grad(&x);
    Err(OptimizeFault::BudgetExhausted {
        iterations: options.max_iterations,
        gradient_inf_norm: gradient_inf_norm(&g),
    })
}

/// BFGS quasi-Newton: the same contract as [`newton`] without a Hessian
/// — the INVERSE-Hessian approximation starts at the identity and gets
/// the rank-2 BFGS update whenever the curvature condition
/// `yᵀs > 1e-10` holds (skipping the update keeps the approximation
/// positive definite instead of corrupting it). Same
/// stationarity/budget/stall honesty as Newton.
pub fn bfgs(
    f: &dyn Fn(&[f64]) -> f64,
    grad: &dyn Fn(&[f64]) -> Vec<f64>,
    x0: &[f64],
    options: &SolverOptions,
) -> Result<SolverOutcome, OptimizeFault> {
    let n = x0.len();
    let mut x = x0.to_vec();
    let mut h_inv = vec![vec![0.0_f64; n]; n];
    for (i, row) in h_inv.iter_mut().enumerate() {
        row[i] = 1.0;
    }
    for iteration in 0..options.max_iterations {
        let g = grad(&x);
        let norm = gradient_inf_norm(&g);
        if norm <= options.tolerance {
            return Ok(SolverOutcome {
                gradient_inf_norm: norm,
                objective: f(&x),
                x,
                iterations: iteration,
            });
        }
        // d = −H ∇f with H the inverse-Hessian approximation.
        let direction: Vec<f64> = (0..n)
            .map(|i| {
                -h_inv[i]
                    .iter()
                    .zip(g.iter())
                    .map(|(h_ij, g_j)| h_ij * g_j)
                    .sum::<f64>()
            })
            .collect();
        let slope: f64 = g.iter().zip(direction.iter()).map(|(gi, di)| gi * di).sum();
        // While H stays positive definite, d is a descent direction
        // (slope < 0). A non-negative slope is numerical breakdown —
        // refuse rather than step uphill.
        if slope >= 0.0 {
            return Err(OptimizeFault::LineSearchStalled { iteration });
        }
        let Some((t, _value)) = line_search(f, &x, slope, &direction) else {
            return Err(OptimizeFault::LineSearchStalled { iteration });
        };
        let step: Vec<f64> = direction.iter().map(|di| t * di).collect();
        let x_new: Vec<f64> = x.iter().zip(step.iter()).map(|(xi, si)| xi + si).collect();
        let g_new = grad(&x_new);
        let y: Vec<f64> = g_new.iter().zip(g.iter()).map(|(gn, go)| gn - go).collect();
        let sy: f64 = step.iter().zip(y.iter()).map(|(si, yi)| si * yi).sum();
        if sy > 1e-10 {
            // Rank-2 BFGS update of the inverse approximation:
            // H ← H − (s yᵀ H + H y sᵀ)/(sᵀy) + (1 + yᵀ H y/(sᵀy))·s sᵀ/(sᵀy).
            let hy: Vec<f64> = (0..n)
                .map(|i| {
                    h_inv[i]
                        .iter()
                        .zip(y.iter())
                        .map(|(h_ij, y_j)| h_ij * y_j)
                        .sum::<f64>()
                })
                .collect();
            let y_hy: f64 = y.iter().zip(hy.iter()).map(|(yi, hy_i)| yi * hy_i).sum();
            let factor = 1.0 + y_hy / sy;
            for i in 0..n {
                for j in 0..n {
                    h_inv[i][j] +=
                        factor * step[i] * step[j] / sy - (step[i] * hy[j] + hy[i] * step[j]) / sy;
                }
            }
        }
        x = x_new;
    }
    let g = grad(&x);
    Err(OptimizeFault::BudgetExhausted {
        iterations: options.max_iterations,
        gradient_inf_norm: gradient_inf_norm(&g),
    })
}

/// KKT residual for an equality-constrained stationary-point claim:
/// `‖(∇f(x) + Σ λᵢ ∇cᵢ(x), c₁(x), …, cₘ(x))‖_∞`. Zero only at a KKT
/// point; stationarity drift and infeasibility both register. This is
/// the certificate helper the constrained methods (and the KKT smoke
/// tests) build on.
pub fn kkt_residual(
    objective_gradient: &[f64],
    constraint_gradients: &[Vec<f64>],
    multipliers: &[f64],
    constraint_values: &[f64],
) -> f64 {
    let n = objective_gradient.len();
    let mut worst = 0.0_f64;
    for i in 0..n {
        let mut lagrangian_i = objective_gradient[i];
        for (grad_c, lambda) in constraint_gradients.iter().zip(multipliers.iter()) {
            lagrangian_i += lambda * grad_c[i];
        }
        worst = worst.max(lagrangian_i.abs());
    }
    for &c in constraint_values {
        worst = worst.max(c.abs());
    }
    worst
}

/// Interior-point (barrier) methods for constrained problems — **not
/// implemented**: the constrained carrier (constraints sections beyond
/// the quadratic penalty, log-barrier schedule, dual slacks) is the
/// named follow-up. Refusing is the contract: a constrained problem
/// must never silently fall back to the unconstrained engine.
pub fn interior_point() -> Result<SolverOutcome, String> {
    Err(
        "interior_point is not implemented: log-barrier constrained methods are the named \
         follow-up; a constrained problem must not silently \
         fall back to the unconstrained engine"
            .into(),
    )
}

/// Sequential quadratic programming — **not implemented**: SQP needs
/// the QP subproblem carrier (equality/active-set machinery), which
/// lands with the constraints work. Refusing is the contract.
pub fn sqp() -> Result<SolverOutcome, String> {
    Err(
        "SQP is not implemented: the QP-subproblem carrier (active-set/equality machinery) is \
         the named follow-up; a constrained problem must not \
         silently fall back to the unconstrained engine"
            .into(),
    )
}
