//! `emath-xx0x.6`: continuous optimization nucleus — contract tests.
//!
//! Newton (with Armijo backtracking line search) and BFGS
//! (quasi-Newton, no Hessian required) as the emath-core reference
//! engines; KKT residual helper for the constrained story; interior-
//! point and SQP refuse by name (phased landing per the bead). The
//! admitted `minimize`/`maximize` goal surface (pure Newton + penalty,
//! exec-ir) is unchanged — goal-surface method SELECTION over this
//! nucleus is the declared wiring follow-up (exec-ir is a foreign hold
//! this tick).
//!
//! Honesty boundaries under test: methods find STATIONARY points (a
//! local claim, never global optimality); a singular Hessian refuses;
//! an exhausted budget refuses with the achieved gradient (a
//! non-solution is never dressed as a result).
//!
//! Failure-first: RED (E0432) until `optimization` lands.

use emath_core::optimization::{
    bfgs, interior_point, kkt_residual, newton, sqp, OptimizeFault, SolverOptions,
};

/// Rosenbrock (a=1, b=100): banana-valley classic; global min at (1,1).
fn rosenbrock(x: &[f64]) -> f64 {
    let a = 1.0;
    let b = 100.0;
    (a - x[0]).powi(2) + b * (x[1] - x[0] * x[0]).powi(2)
}

fn rosenbrock_grad(x: &[f64]) -> Vec<f64> {
    vec![
        -2.0 * (1.0 - x[0]) - 400.0 * x[0] * (x[1] - x[0] * x[0]),
        200.0 * (x[1] - x[0] * x[0]),
    ]
}

fn rosenbrock_hess(x: &[f64]) -> Vec<Vec<f64>> {
    vec![
        vec![1200.0 * x[0] * x[0] - 400.0 * x[1] + 2.0, -400.0 * x[0]],
        vec![-400.0 * x[0], 200.0],
    ]
}

#[test]
fn newton_minimizes_rosenbrock() {
    let outcome = newton(
        &rosenbrock,
        &rosenbrock_grad,
        &rosenbrock_hess,
        &[-1.2, 1.0],
        &SolverOptions::default(),
    )
    .unwrap();
    assert!((outcome.x[0] - 1.0).abs() < 1e-5);
    assert!((outcome.x[1] - 1.0).abs() < 1e-5);
    assert!(outcome.gradient_inf_norm <= 1e-8, "converged ‖∇f‖ bound");
    assert!(outcome.iterations < 200, "budget not the convergence mechanism");
    assert!((outcome.objective - rosenbrock(&outcome.x)).abs() < 1e-15, "objective is f(x)");
}

#[test]
fn newton_singular_hessian_refuses() {
    // f(x, y) = x²: flat in y, so the Hessian is singular everywhere —
    // the solve is impossible and must be refused BY NAME, never
    // solved against a pseudo-inverse.
    let flat = |x: &[f64]| x[0] * x[0];
    let flat_grad = |x: &[f64]| vec![2.0 * x[0], 0.0];
    let flat_hess = |_x: &[f64]| vec![vec![2.0, 0.0], vec![0.0, 0.0]];
    let fault = newton(&flat, &flat_grad, &flat_hess, &[1.0, 1.0], &SolverOptions::default())
        .unwrap_err();
    assert!(
        matches!(fault, OptimizeFault::SingularHessian { .. }),
        "singular Hessian must be named, got {fault:?}"
    );
}

#[test]
fn newton_budget_exhaustion_refuses_with_gradient() {
    // A 2-iteration budget cannot finish Rosenbrock; the refusal must
    // carry the achieved ‖∇f‖ — evidence of how far it got, never a
    // dressed-up non-solution.
    let fault = newton(
        &rosenbrock,
        &rosenbrock_grad,
        &rosenbrock_hess,
        &[-1.2, 1.0],
        &SolverOptions {
            tolerance: 1e-8,
            max_iterations: 2,
        },
    )
    .unwrap_err();
    match fault {
        OptimizeFault::BudgetExhausted {
            iterations,
            gradient_inf_norm,
        } => {
            assert_eq!(iterations, 2);
            assert!(gradient_inf_norm > 1e-8, "achieved gradient must be > tolerance");
        }
        other => panic!("budget refusal, got {other:?}"),
    }
}

#[test]
fn bfgs_minimizes_rosenbrock_without_hessian() {
    // The quasi-Newton point: only objective + gradient available.
    let outcome = bfgs(
        &rosenbrock,
        &rosenbrock_grad,
        &[-1.2, 1.0],
        &SolverOptions::default(),
    )
    .unwrap();
    assert!((outcome.x[0] - 1.0).abs() < 1e-5);
    assert!((outcome.x[1] - 1.0).abs() < 1e-5);
}

#[test]
fn bfgs_matches_newton_on_quadratic() {
    // Convex quadratic: both methods land on the same minimizer
    // (Newton in one step modulo line search; BFGS in ≤ n steps).
    let f = |x: &[f64]| (x[0] - 1.0).powi(2) + 4.0 * (x[1] - 2.0).powi(2);
    let grad = |x: &[f64]| vec![2.0 * (x[0] - 1.0), 8.0 * (x[1] - 2.0)];
    let hess = |_x: &[f64]| vec![vec![2.0, 0.0], vec![0.0, 8.0]];
    let n = newton(&f, &grad, &hess, &[0.0, 0.0], &SolverOptions::default()).unwrap();
    let q = bfgs(&f, &grad, &[0.0, 0.0], &SolverOptions::default()).unwrap();
    assert!((n.x[0] - 1.0).abs() < 1e-8 && (n.x[1] - 2.0).abs() < 1e-8);
    assert!((q.x[0] - 1.0).abs() < 1e-6 && (q.x[1] - 2.0).abs() < 1e-6);
}

#[test]
fn bfgs_survives_curvature_violation_on_nonconvex() {
    // f(x) = sin(x) from 0.5: the first Armijo step (t=1) lands at
    // −0.3776, where y = ∇f_new − ∇f ≈ +0.052 against s ≈ −0.878 gives
    // yᵀs < 0 — the curvature condition FAILS. Honesty contract: the
    // update must be SKIPPED (keeping H positive definite), and the
    // solver still converges to the stationary point. A mutant that
    // applies the update anyway corrupts H negative, making the next
    // direction an ASCENT — which the slope guard refuses (the pin
    // fails). This is the pin that kills the dropped-guard mutant.
    let outcome = bfgs(
        &|x: &[f64]| x[0].sin(),
        &|x: &[f64]| vec![x[0].cos()],
        &[0.5],
        &SolverOptions::default(),
    )
    .unwrap();
    assert!(
        outcome.gradient_inf_norm <= 1e-8,
        "must reach a stationary point of sin, got ‖∇‖ = {}",
        outcome.gradient_inf_norm
    );
    assert!(
        (outcome.objective - (-1.0)).abs() < 1e-6,
        "sin descends to its minimum −1 here, got {}",
        outcome.objective
    );
}

#[test]
fn bfgs_budget_exhaustion_refuses() {
    let fault = bfgs(
        &rosenbrock,
        &rosenbrock_grad,
        &[-1.2, 1.0],
        &SolverOptions {
            tolerance: 1e-8,
            max_iterations: 3,
        },
    )
    .unwrap_err();
    assert!(
        matches!(fault, OptimizeFault::BudgetExhausted { .. }),
        "budget refusal, got {fault:?}"
    );
}

#[test]
fn kkt_residual_zero_at_known_solution() {
    // min x²+y² s.t. x+y=2. L = f + λ(x+y−2); stationarity 2x+λ = 0,
    // 2y+λ = 0, feasibility x+y−2 = 0 → (x, y, λ) = (1, 1, −2).
    let residual = kkt_residual(
        &[2.0, 2.0],
        &[vec![1.0, 1.0]],
        &[-2.0],
        &[0.0],
    );
    assert!(residual.abs() < 1e-12, "KKT point has residual 0, got {residual}");
}

#[test]
fn kkt_residual_detects_stationarity_and_feasibility_drift() {
    // Wrong multipliers leave a stationarity residual; wrong point
    // leaves a feasibility residual. Both must register.
    let stationarity = kkt_residual(&[2.0, 2.0], &[vec![1.0, 1.0]], &[-1.0], &[0.0]);
    assert!(stationarity > 0.5, "wrong λ must register, got {stationarity}");
    let feasibility = kkt_residual(&[2.0, 2.0], &[vec![1.0, 1.0]], &[-2.0], &[0.25]);
    assert!(feasibility > 0.1, "infeasibility must register, got {feasibility}");
}

#[test]
fn interior_point_and_sqp_refuse_by_name() {
    // Phased landing: the constrained solvers refuse with the method
    // NAMED and the follow-up named — never a silent fallback to the
    // unconstrained engine on a constrained problem.
    let ip = interior_point().unwrap_err();
    assert!(
        ip.contains("interior_point") && ip.contains("not implemented"),
        "IP refusal must name the method, got {ip}"
    );
    let sqp_error = sqp().unwrap_err();
    assert!(
        sqp_error.contains("SQP") && sqp_error.contains("not implemented"),
        "SQP refusal must name the method, got {sqp_error}"
    );
}

#[test]
fn newton_refuses_non_descent_direction() {
    // f(x) = ln(1+x²) has NEGATIVE curvature for |x| > 1, so at x=2 the
    // Newton step points AWAY from the minimum (toward the max): the
    // slope gᵀd is positive, no Armijo decrease exists along it, and
    // the honest behavior is a named refusal — not stepping uphill.
    let f = |x: &[f64]| (1.0 + x[0] * x[0]).ln();
    let grad = |x: &[f64]| vec![2.0 * x[0] / (1.0 + x[0] * x[0])];
    let hess = |x: &[f64]| {
        let d = 1.0 + x[0] * x[0];
        vec![vec![2.0 * (1.0 - x[0] * x[0]) / (d * d)]]
    };
    let fault = newton(&f, &grad, &hess, &[2.0], &SolverOptions::default()).unwrap_err();
    assert!(
        matches!(fault, OptimizeFault::LineSearchStalled { .. }),
        "ascent direction must refuse, got {fault:?}"
    );
}

#[test]
fn newton_minimizes_ill_conditioned_quadratic() {
    // f = ½xᵀAx with A = [[1e-8, 9e-5],[9e-5, 1]] (det = 1.9e-9 > 0,
    // cond ≈ 5e8) from x₀ = [1,1]. Oracle note (disclosed): the first
    // draft used b = 1e-4 — exactly det 0, a rank-1 matrix — and
    // correctly refused on the REAL code too; a singular-system pin
    // was not the intent. Nonsingular but ill-conditioned: Newton must
    // still land on x ≈ 0 with ‖∇f‖ ≤ tol. (Mutation note: the
    // remove-pivoting mutant survives this suite — for PSD systems
    // no-pivot elimination has bounded growth, and indefinite systems
    // never reach the solve productively — so pivoting is documented
    // as the standard-choice equivalent mutant over this envelope.)
    let f = |x: &[f64]| {
        0.5 * (1e-8 * x[0] * x[0] + 2.0 * 9e-5 * x[0] * x[1] + x[1] * x[1])
    };
    let grad = |x: &[f64]| {
        vec![1e-8 * x[0] + 9e-5 * x[1], 9e-5 * x[0] + x[1]]
    };
    let hess = |_x: &[f64]| vec![vec![1e-8, 9e-5], vec![9e-5, 1.0]];
    let outcome = newton(&f, &grad, &hess, &[1.0, 1.0], &SolverOptions::default()).unwrap();
    assert!(outcome.x[0].abs() < 1e-4 && outcome.x[1].abs() < 1e-6);
    assert!(outcome.gradient_inf_norm <= 1e-8);
}

#[test]
fn tolerance_semantics_are_gradient_based() {
    // Convergence is declared as ‖∇f‖_inf ≤ tolerance (stationarity),
    // NOT objective-change or step-size: a large-flat-basin point must
    // not read as converged while the gradient is still large, and a
    // tiny-gradient point with big objective change still converges.
    let shallow = |x: &[f64]| 1e-3 * x[0] * x[0];
    let shallow_grad = |x: &[f64]| vec![2e-3 * x[0]];
    let hess = |_x: &[f64]| vec![vec![2e-3]];
    // From x=100: gradient starts at 0.2 > default tol → must NOT
    // return unconverged as solved; budget runs then refuses.
    let fault = newton(
        &shallow,
        &shallow_grad,
        &hess,
        &[100.0],
        &SolverOptions {
            tolerance: 1e-8,
            max_iterations: 500,
        },
    );
    match fault {
        Ok(outcome) => assert!(outcome.gradient_inf_norm <= 1e-8),
        Err(OptimizeFault::BudgetExhausted { gradient_inf_norm, .. }) => {
            assert!(gradient_inf_norm > 1e-8);
        }
        Err(other) => panic!("unexpected fault {other:?}"),
    }
}
