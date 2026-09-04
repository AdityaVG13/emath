//! Simulation configuration and result types.

use super::*;

/// Bisection budget shared by the two event locators (the `--event`
/// variable tracker and the event-firing tracker): the `Trajectory`
/// docs promise "the fixed 40-iteration budget" for both, so the
/// constant — not a copied literal — carries that promise.
pub(super) const EVENT_LOCATE_ITERATIONS: usize = 40;
/// Bisection convergence tolerance on time (seconds).
pub(super) const EVENT_LOCATE_TOLERANCE: f64 = 1e-12;

/// Explicit first-order stepper for `emath model` rates stored as `der_<state>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepMethod {
    /// Forward Euler: `x += h * f(x)`.
    Euler,
    /// Classic RK4.
    Rk4,
    /// Cash-Karp RK45. Fixed-step uses the 5th-order update; adaptive
    /// mode compares 4th vs 5th and accepts/rejects the step.
    Rk45,
    /// Implicit (backward) Euler (runner slice): Newton on the
    /// residual `r(x₁) = x₁ − x_n − h·f(x₁)` — the stiff-stable
    /// sibling of `Euler` where explicit stepping diverges. Scalar
    /// differential state per the nucleus carrier; non-convergence
    /// refuses `E-ODE-001`, non-positive `dt` refuses `E-ODE-003`.
    BackwardEuler,
    /// Velocity Verlet (runner slice): kick-drift-kick for the
    /// separable system `q' = v`, `v' = a(q)` — one rate evaluation
    /// pair per step, time-reversible (`h` may be negative). The
    /// STRUCTURE gate refuses `E-ODE-002` when the model is not
    /// separable in that shape: symplectic integrators preserve
    /// structure only for structure-preserving problems.
    VelocityVerlet,
}

/// Optional adaptive / event controls. Absent tolerances keep fixed `dt`.
#[derive(Clone, Debug, PartialEq)]
pub struct SimulateOptions {
    pub atol: Option<f64>,
    pub rtol: Option<f64>,
    pub dt_max: Option<f64>,
    /// Stop at the first crossing of `state[name] - value`.
    pub event: Option<(String, f64)>,
}

impl Default for SimulateOptions {
    fn default() -> Self {
        Self {
            atol: None,
            rtol: None,
            dt_max: None,
            event: None,
        }
    }
}

impl SimulateOptions {
    pub(super) fn adaptive(&self) -> bool {
        self.atol.is_some() || self.rtol.is_some()
    }
}

/// One sample on a simulated trajectory.
///
/// For causalized implicit DAEs the map holds the differential state and
/// the projected `algebraic:` values, so the algebraic residual at the
/// sample is ~0 after a successful step (index-1 projection). ODE models
/// have only differential keys.
#[derive(Clone, Debug, PartialEq)]
pub struct TrajectorySample {
    pub t: f64,
    pub state: BTreeMap<String, Value>,
}

/// Explicit trajectory from `t0` through `t1` at step `dt`.
#[derive(Clone, Debug, PartialEq)]
pub struct Trajectory {
    pub method: StepMethod,
    pub dt: f64,
    pub samples: Vec<TrajectorySample>,
    /// Hybrid events fired during the run
    /// (ch7, event-execution slice), in firing order.
    /// Deterministic: conditions are evaluated once per accepted step,
    /// one event fires per rising edge per step, ties break in
    /// declaration order, and the crossing time is bisected within the
    /// fixed 40-iteration budget (same budget as `--event` location).
    /// Empty for models with no `events:` payloads.
    pub events: Vec<EventFiring>,
}

/// One fired hybrid event: name plus the crossing time.
#[derive(Clone, Debug, PartialEq)]
pub struct EventFiring {
    pub name: String,
    pub t: f64,
}

/// Structural index class of the simulated system. `One` is the
/// causalized-algebraic slice the Newton solver actually handles; higher
/// indexes are not claimed by the native path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DAEIndex {
    /// No `algebraic:` unknowns — a plain ODE.
    Ode,
    /// `algebraic:` unknowns solvable by the causalized Newton solve at
    /// every step (index ≤ 1 after the causalization the admission
    /// already performs).
    One,
}

/// Consistent-initialization verdict from the t0 algebraic projection
/// check: did the constraint manifold accept the initial state?
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InitializationVerdict {
    /// The t0 projection converged: max |algebraic residual| ≤ 1e-6.
    Consistent,
}

/// One continuation action when the disposition refuses. The record
/// says what to DO next — never a bare error string.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Continuation {
    /// Supply a start guess for the named algebraic unknown(s) so the
    /// t0 Newton solve can run.
    SupplyInitialGuess {
        /// The unknown(s) missing a guess.
        names: Vec<String>,
    },
    /// The residual system is structurally singular for these inputs
    /// (some unknown left the residual or the Jacobian lost rank);
    /// regularize the equations or fix the input values.
    Regularize {
        /// What the solver observed (diagnostic detail, deterministic).
        detail: String,
    },
}

/// The disposition record beside a trajectory: structural
/// index, constraint/differential partition, initialization verdict,
/// and — on refusal — the continuation. Present on EVERY simulate run
/// (ODE models get `DAEIndex::Ode`), so a consumer can never receive a
/// naked trajectory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DAEDisposition {
    pub index: DAEIndex,
    /// Differential state names (integrated).
    pub differential_states: Vec<String>,
    /// `algebraic:` unknown names (projected, not integrated).
    pub constraint_unknowns: Vec<String>,
    pub initialization: InitializationVerdict,
    /// `Some` only when the run refused; the trajectory then does not
    /// exist. Empty on success.
    pub continuation: Option<Continuation>,
}
