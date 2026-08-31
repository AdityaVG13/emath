//! ODE simulation and integration machinery: explicit steppers
//! (Euler / RK4 / Cash-Karp RK45), adaptive dt, event location, and
//! causalized implicit-DAE Newton solving.

mod newton;
use newton::causal_newton;

use crate::interp::Value;
use crate::EmirExprRef;
use emath_ir::{Declaration, EventDecl, SemanticPackage, TransitionDecl};
use std::collections::{BTreeMap, BTreeSet};

/// Bisection budget shared by the two event locators (the `--event`
/// variable tracker and the event-firing tracker): the `Trajectory`
/// docs promise "the fixed 40-iteration budget" for both, so the
/// constant — not a copied literal — carries that promise.
const EVENT_LOCATE_ITERATIONS: usize = 40;
/// Bisection convergence tolerance on time (seconds).
const EVENT_LOCATE_TOLERANCE: f64 = 1e-12;

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
    /// Implicit (backward) Euler (xx0x.3 runner slice): Newton on the
    /// residual `r(x₁) = x₁ − x_n − h·f(x₁)` — the stiff-stable
    /// sibling of `Euler` where explicit stepping diverges. Scalar
    /// differential state per the nucleus carrier; non-convergence
    /// refuses `E-ODE-001`, non-positive `dt` refuses `E-ODE-003`.
    BackwardEuler,
    /// Velocity Verlet (xx0x.3 runner slice): kick-drift-kick for the
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
    fn adaptive(&self) -> bool {
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
    /// (r3-dynamical-03lh ch7, event-execution slice), in firing order.
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

/// Structural index class of the simulated system (b9flv). `One` is the
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
/// (b9flv): did the constraint manifold accept the initial state?
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

/// The disposition record beside a trajectory (b9flv): structural
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

/// Advance one explicit step. Rates come from admitted `der_<name>` definitions.
pub fn step_continuous(
    package: &SemanticPackage,
    declaration: &Declaration,
    inputs: &BTreeMap<String, f64>,
    state: &BTreeMap<String, f64>,
    dt: f64,
    method: StepMethod,
) -> Result<BTreeMap<String, f64>, String> {
    let inputs = scalar_map_to_values(inputs);
    let state = scalar_map_to_values(state);
    let next = step_continuous_values(package, declaration, &inputs, &state, dt, method)?;
    values_to_scalars(&next)
}

/// Advance one explicit step, allowing vector-valued state and rates.
pub fn step_continuous_values(
    package: &SemanticPackage,
    declaration: &Declaration,
    inputs: &BTreeMap<String, Value>,
    state: &BTreeMap<String, Value>,
    dt: f64,
    method: StepMethod,
) -> Result<BTreeMap<String, Value>, String> {
    if !dt.is_finite() || dt == 0.0 {
        return Err(format!(
            "E-ODE-003: step size must be a finite non-zero Float64 (a non-advancing step \
             must never return the input as an integrated value), got {dt}"
        ));
    }
    // Negative `dt` is legal ONLY on the time-reversible symplectic
    // path (velocity Verlet); every other stepper is forward-only.
    if dt < 0.0 && method != StepMethod::VelocityVerlet {
        return Err(format!(
            "E-ODE-003: step size must be a positive finite Float64 for {method:?} (negative \
             dt is the velocity-Verlet time-reversal contract), got {dt}"
        ));
    }
    let skip = algebraic_name_set(declaration);
    let mut next = match method {
        StepMethod::Euler => {
            let rates = eval_rates(package, declaration, inputs, state)?;
            apply_scaled(state, &[(1.0, &rates)], dt, &skip)?
        }
        StepMethod::BackwardEuler => {
            implicit_backward_euler_step(package, declaration, inputs, state, dt, &skip)?
        }
        StepMethod::VelocityVerlet => {
            velocity_verlet_step(package, declaration, inputs, state, dt)?
        }
        StepMethod::Rk4 => {
            let k1 = eval_rates(package, declaration, inputs, state)?;
            let s2 = apply_scaled(state, &[(1.0, &k1)], dt / 2.0, &skip)?;
            let k2 = eval_rates(package, declaration, inputs, &s2)?;
            let s3 = apply_scaled(state, &[(1.0, &k2)], dt / 2.0, &skip)?;
            let k3 = eval_rates(package, declaration, inputs, &s3)?;
            let s4 = apply_scaled(state, &[(1.0, &k3)], dt, &skip)?;
            let k4 = eval_rates(package, declaration, inputs, &s4)?;
            apply_scaled(
                state,
                &[
                    (1.0 / 6.0, &k1),
                    (2.0 / 6.0, &k2),
                    (2.0 / 6.0, &k3),
                    (1.0 / 6.0, &k4),
                ],
                dt,
                &skip,
            )?
        }
        StepMethod::Rk45 => {
            let stages = cash_karp_stages(package, declaration, inputs, state, dt)?;
            stages.fifth
        }
    };
    project_algebraic_into(package, declaration, inputs, &mut next)?;
    Ok(next)
}

/// Integrate from `t0` to `t1` with fixed `dt`. Includes the sample at `t0`.
pub fn simulate_continuous(
    package: &SemanticPackage,
    declaration: &Declaration,
    inputs: &BTreeMap<String, Value>,
    state: &BTreeMap<String, Value>,
    t0: f64,
    t1: f64,
    dt: f64,
    method: StepMethod,
) -> Result<Trajectory, String> {
    simulate_continuous_with(
        package,
        declaration,
        inputs,
        state,
        t0,
        t1,
        dt,
        method,
        &SimulateOptions::default(),
    )
}

/// Integrate from `t0` to `t1`. Adaptive dt and one event locator are optional.
pub fn simulate_continuous_with(
    package: &SemanticPackage,
    declaration: &Declaration,
    inputs: &BTreeMap<String, Value>,
    state: &BTreeMap<String, Value>,
    t0: f64,
    t1: f64,
    dt: f64,
    method: StepMethod,
    options: &SimulateOptions,
) -> Result<Trajectory, String> {
    simulate_continuous_dispositioned(
        package,
        declaration,
        inputs,
        state,
        t0,
        t1,
        dt,
        method,
        options,
    )
    .map(|(trajectory, _disposition)| trajectory)
}

/// `simulate_continuous_with` plus the b9flv disposition record: the
/// structural index, the constraint/differential partition, the t0
/// initialization verdict — or a typed refusal with a continuation
/// note when the constraint cannot be honored (never a silent ODE drop
/// of the algebraic equations, never a trajectory pretending the
/// initialization succeeded).
pub fn simulate_continuous_dispositioned(
    package: &SemanticPackage,
    declaration: &Declaration,
    inputs: &BTreeMap<String, Value>,
    state: &BTreeMap<String, Value>,
    t0: f64,
    t1: f64,
    dt: f64,
    method: StepMethod,
    options: &SimulateOptions,
) -> Result<(Trajectory, DAEDisposition), String> {
    let index = if declaration.algebraic.is_empty() {
        DAEIndex::Ode
    } else {
        DAEIndex::One
    };
    let differential_states: Vec<String> = declaration
        .state
        .iter()
        .map(|field| field.name.clone())
        .collect();
    let constraint_unknowns: Vec<String> = declaration
        .algebraic
        .iter()
        .map(|field| field.name.clone())
        .collect();
    // Consistent initialization IS the t0 projection: run it up front so
    // a failure refuses with a continuation before any trajectory is
    // built. `project_algebraic_into` also runs inside the first step;
    // doing it here first makes the verdict first-class.
    if index == DAEIndex::One {
        if let Err(projection_error) =
            project_algebraic_into(package, declaration, inputs, &mut state.clone())
        {
            let continuation = classify_projection_failure(&projection_error, &constraint_unknowns);
            return Err(disposition_refusal(
                &continuation,
                &differential_states,
                &constraint_unknowns,
            ));
        }
    }
    let trajectory = simulate_continuous_inner(
        package,
        declaration,
        inputs,
        state,
        t0,
        t1,
        dt,
        method,
        options,
    )?;
    let disposition = DAEDisposition {
        index,
        differential_states,
        constraint_unknowns,
        initialization: InitializationVerdict::Consistent,
        continuation: None,
    };
    Ok((trajectory, disposition))
}

/// Map a t0 projection error to its continuation action.
fn classify_projection_failure(error: &str, constraint_unknowns: &[String]) -> Continuation {
    if error.contains("missing algebraic-variable guess") || error.contains("missing input") {
        Continuation::SupplyInitialGuess {
            names: constraint_unknowns.to_vec(),
        }
    } else {
        Continuation::Regularize {
            detail: error.to_string(),
        }
    }
}

/// The typed refusal text: E-DAE-INIT code, the verdict, and the
/// continuation — a consumer can act on it without re-deriving.
fn disposition_refusal(
    continuation: &Continuation,
    differential_states: &[String],
    constraint_unknowns: &[String],
) -> String {
    let action = match continuation {
        Continuation::SupplyInitialGuess { names } => format!(
            "supply an initial guess for the algebraic unknown(s) `{}` (add them to the \
             simulate inputs map)",
            names.join("`, `")
        ),
        Continuation::Regularize { detail } => {
            format!("regularize the residual system before integrating ({detail})")
        }
    };
    format!(
        "E-DAE-INIT: consistent initialization failed; the constraint (algebraic unknowns: {}) \
         was NOT dropped and no trajectory is presented. Differential states ({}). Continuation: {action}",
        if constraint_unknowns.is_empty() {
            "none".to_string()
        } else {
            constraint_unknowns.join(", ")
        },
        if differential_states.is_empty() {
            "none".to_string()
        } else {
            differential_states.join(", ")
        },
    )
}

/// The integration loop with generic hybrid event scheduling
/// (r3-dynamical-03lh ch7, event-execution slice). Event actions are
/// applied to the LIVE input/state maps so later steps see them; the
/// caller's maps are never mutated. With no `events:` payloads this is
/// the pre-event loop verbatim.
fn simulate_continuous_inner(
    package: &SemanticPackage,
    declaration: &Declaration,
    inputs: &BTreeMap<String, Value>,
    state: &BTreeMap<String, Value>,
    t0: f64,
    t1: f64,
    dt: f64,
    method: StepMethod,
    options: &SimulateOptions,
) -> Result<Trajectory, String> {
    if !t0.is_finite() || !t1.is_finite() {
        return Err("trajectory times must be finite Float64".to_string());
    }
    if t1 < t0 {
        return Err("trajectory end must be >= start".to_string());
    }
    if !dt.is_finite() || dt <= 0.0 {
        return Err("step size must be a positive finite Float64".to_string());
    }
    if let Some(atol) = options.atol {
        if !atol.is_finite() || atol <= 0.0 {
            return Err("atol must be a positive finite Float64".to_string());
        }
    }
    if let Some(rtol) = options.rtol {
        if !rtol.is_finite() || rtol <= 0.0 {
            return Err("rtol must be a positive finite Float64".to_string());
        }
    }
    if let Some(dt_max) = options.dt_max {
        if !dt_max.is_finite() || dt_max <= 0.0 {
            return Err("dt-max must be a positive finite Float64".to_string());
        }
    }
    if options.adaptive() && method != StepMethod::Rk45 {
        return Err("adaptive dt requires --method rk45".to_string());
    }
    // Event actions persist into subsequent steps, so the runner keeps a
    // live input map; the caller's map stays untouched.
    let mut live_inputs = inputs.clone();
    let mut events_fired: Vec<EventFiring> = Vec::new();
    let events: &[EventDecl] = package
        .events
        .get(&declaration.id)
        .map(|events| events.as_slice())
        .unwrap_or(&[]);
    let transitions: &[TransitionDecl] = package
        .transitions
        .get(&declaration.id)
        .map(|transitions| transitions.as_slice())
        .unwrap_or(&[]);
    let mut current = state.clone();
    project_algebraic_into(package, declaration, &live_inputs, &mut current)?;
    // Events whose condition already holds at the initial sample fire at
    // t0, in declaration order — a switch that is closed at start-up.
    fire_t0_events(
        package,
        declaration,
        events,
        transitions,
        &mut live_inputs,
        &mut current,
        t0,
        &mut events_fired,
    )?;
    let mut samples = vec![TrajectorySample {
        t: t0,
        state: current.clone(),
    }];
    let mut t = t0;
    let mut h = match options.dt_max {
        Some(dt_max) => dt.min(dt_max),
        None => dt,
    };
    let mut guard = 0_u32;
    const MAX_STEPS: u32 = 1_000_000;
    while t < t1 {
        let remaining = t1 - t;
        if remaining <= 0.0 {
            break;
        }
        let step = h.min(remaining);
        if step <= 0.0 {
            break;
        }
        let (next, used, err) = if options.adaptive() {
            adaptive_rk45_try(package, declaration, &live_inputs, &current, step, options)?
        } else {
            let next = step_continuous_values(
                package,
                declaration,
                &live_inputs,
                &current,
                step,
                method,
            )?;
            (next, step, 0.0)
        };
        // Finite-state invariant, matching the adaptive path (which
        // checks `values_finite` inside `adaptive_rk45_try`): a step
        // that produces NaN/±Inf fails the run — it must never silently
        // poison every later sample of the trajectory.
        if !values_finite(&next) {
            return Err(format!(
                "step produced a non-finite state at t={t}: values diverged \
                 (reduce dt or use a stiff/implicit method)"
            ));
        }
        if options.adaptive() && used < step && used <= 0.0 {
            return Err("adaptive step collapsed to a non-positive dt".to_string());
        }
        if options.adaptive() && used < step * 0.999 {
            h = used;
            guard += 1;
            if guard > MAX_STEPS {
                return Err("trajectory exceeded 1_000_000 steps".to_string());
            }
            continue;
        }
        let mut next = next;
        if options.adaptive() {
            project_algebraic_into(package, declaration, &live_inputs, &mut next)?;
        }
        // Hybrid events (r3-dynamical-03lh ch7): the FIRST event in
        // declaration order whose condition rises across this accepted
        // step fires at its bisected crossing; the action applies to the
        // live maps and later steps see it. At most one event per step —
        // the deterministic tie-break and the per-step budget.
        if let Some(fire_t) = try_fire_rising_event(
            package,
            declaration,
            events,
            transitions,
            &mut live_inputs,
            &mut current,
            &next,
            t,
            used,
            method,
            &mut events_fired,
        )? {
            t = fire_t;
            samples.push(TrajectorySample {
                t,
                state: current.clone(),
            });
            guard += 1;
            if guard > MAX_STEPS {
                return Err("trajectory exceeded 1_000_000 steps".to_string());
            }
            continue;
        }
        if let Some((name, value)) = &options.event {
            if let Some((event_t, event_state)) = locate_event(
                package,
                declaration,
                &live_inputs,
                &current,
                &next,
                t,
                used,
                name,
                *value,
                method,
            )? {
                samples.push(TrajectorySample {
                    t: event_t,
                    state: event_state,
                });
                return Ok(Trajectory {
                    method,
                    dt: h,
                    samples,
                    events: events_fired,
                });
            }
        }
        current = next;
        t += used;
        samples.push(TrajectorySample {
            t,
            state: current.clone(),
        });
        if options.adaptive() {
            h = grow_step(h, err, options, remaining);
        }
        guard += 1;
        if guard > MAX_STEPS {
            return Err("trajectory exceeded 1_000_000 steps".to_string());
        }
        if (t - t1).abs() <= h * 1e-12 {
            break;
        }
    }
    Ok(Trajectory {
        method,
        dt: h,
        samples,
        events: events_fired,
    })
}

/// Fire every declared event whose condition is true at the initial
/// sample, in declaration order. Each action applies to the live input
/// map (persists) or the initial state map; after all t0 actions the
/// algebraic projection re-runs so the consistent-initialization
/// verdict stays honest under the switched inputs.
fn fire_t0_events(
    package: &SemanticPackage,
    declaration: &Declaration,
    events: &[EventDecl],
    transitions: &[TransitionDecl],
    live_inputs: &mut BTreeMap<String, Value>,
    current: &mut BTreeMap<String, Value>,
    t0: f64,
    events_fired: &mut Vec<EventFiring>,
) -> Result<(), String> {
    for event in events {
        let held = event_condition(package, declaration, live_inputs, current, event)?;
        if !held {
            continue;
        }
        apply_event_action(package, declaration, live_inputs, current, event, t0)?;
        apply_transition_actions(
            package,
            declaration,
            live_inputs,
            current,
            transitions,
            event,
            t0,
        )?;
        events_fired.push(EventFiring {
            name: event.name.clone(),
            t: t0,
        });
    }
    if !events_fired.is_empty() {
        project_algebraic_into(package, declaration, live_inputs, current)?;
    }
    Ok(())
}

/// Detect the first declared event whose condition rises (false → true)
/// across the accepted step. On a rising edge the crossing time is
/// bisected within the fixed 40-iteration budget — the same budget as
/// the `--event` variable locator — and the action applies at the
/// crossing state. Returns `Some(fire_t)` when an event fired (the loop
/// then continues from the crossing state; `current`, `events_fired`,
/// and the live input map carry the mutation).
fn try_fire_rising_event(
    package: &SemanticPackage,
    declaration: &Declaration,
    events: &[EventDecl],
    transitions: &[TransitionDecl],
    live_inputs: &mut BTreeMap<String, Value>,
    current: &mut BTreeMap<String, Value>,
    next: &BTreeMap<String, Value>,
    t: f64,
    dt: f64,
    method: StepMethod,
    events_fired: &mut Vec<EventFiring>,
) -> Result<Option<f64>, String> {
    for event in events {
        let before = event_condition(package, declaration, live_inputs, current, event)?;
        if before {
            continue;
        }
        let after = event_condition(package, declaration, live_inputs, next, event)?;
        if !after {
            continue;
        }
        // Rising edge: bisect between current and next.
        let mut lo_t = t;
        let mut hi_t = t + dt;
        let mut lo = current.clone();
        let mut hi = next.clone();
        for _ in 0..EVENT_LOCATE_ITERATIONS {
            let mid_t = 0.5 * (lo_t + hi_t);
            let mid = step_continuous_values(
                package,
                declaration,
                live_inputs,
                &lo,
                mid_t - lo_t,
                method,
            )?;
            let held = event_condition(package, declaration, live_inputs, &mid, event)?;
            if held {
                hi_t = mid_t;
                hi = mid;
            } else {
                lo_t = mid_t;
                lo = mid;
            }
            if hi_t - lo_t <= EVENT_LOCATE_TOLERANCE {
                break;
            }
        }
        let fire_t = 0.5 * (lo_t + hi_t);
        let mut fired_state = hi.clone();
        apply_event_action(
            package,
            declaration,
            live_inputs,
            &mut fired_state,
            event,
            fire_t,
        )?;
        apply_transition_actions(
            package,
            declaration,
            live_inputs,
            &mut fired_state,
            transitions,
            event,
            fire_t,
        )?;
        project_algebraic_into(package, declaration, live_inputs, &mut fired_state)?;
        *current = fired_state;
        events_fired.push(EventFiring {
            name: event.name.clone(),
            t: fire_t,
        });
        return Ok(Some(fire_t));
    }
    Ok(None)
}

/// Evaluate an event's condition at a state point. The condition
/// expression was admitted Boolean with definitions inlined; runtime
/// evaluation binds inputs (+ algebraic unknowns from the state map, so
/// the projected values participate) and state fields through the same
/// lowering path as definitions.
fn event_condition(
    package: &SemanticPackage,
    declaration: &Declaration,
    live_inputs: &BTreeMap<String, Value>,
    state: &BTreeMap<String, Value>,
    event: &EventDecl,
) -> Result<bool, String> {
    match eval_event_expr(package, declaration, live_inputs, state, event.condition)? {
        Value::Bool(flag) => Ok(flag),
        other => Err(format!(
            "E-EVENT-006: event `{}` condition must evaluate to Bool, got {other:?}",
            event.name
        )),
    }
}

/// Refuse a non-finite / non-scalar action value: an event/transition
/// action writing NaN/±Inf (or a non-numeric carrier) into a slot would
/// violate the finite-state invariant the runner enforces everywhere
/// else (same discipline as `values_finite` in adaptive stepping). The
/// write is abandoned BEFORE it lands, so a poisoned value never enters
/// the live input or state map. `code` distinguishes the event payload
/// refusal (E-EVENT-009) from the transition refusal (E-TRANS-008);
/// `who` names the event/rule and `target` the slot, both in the text.
fn check_finite_action_value(
    code: &str,
    who: &str,
    target: &str,
    value: &Value,
) -> Result<(), String> {
    match value {
        Value::F64(number) if !number.is_finite() => Err(format!(
            "{code}: {who} must write a finite numeric scalar, got {number}; non-finite action \
             value — target `{target}` was NOT written"
        )),
        Value::F64(_) | Value::I64(_) => Ok(()),
        other => Err(format!(
            "{code}: {who} must write a finite numeric scalar, got {other:?}; non-numeric \
             action value — target `{target}` was NOT written"
        )),
    }
}

/// Evaluate an event's action right-hand side at a state point and
/// write the target slot: an `inputs:` name goes into the live input
/// map (all later steps see it), a `state:` name into the state map.
fn apply_event_action(
    package: &SemanticPackage,
    declaration: &Declaration,
    live_inputs: &mut BTreeMap<String, Value>,
    state: &mut BTreeMap<String, Value>,
    event: &EventDecl,
    fire_t: f64,
) -> Result<(), String> {
    let value = eval_event_expr(package, declaration, live_inputs, state, event.action.expr)?;
    let target = &event.action.target;
    check_finite_action_value("E-EVENT-009", &format!("event `{}` action", event.name), target, &value)?;
    let is_input = declaration.inputs.iter().any(|field| &field.name == target);
    if is_input {
        if live_inputs.contains_key(target) {
            live_inputs.insert(target.clone(), value);
        } else {
            return Err(format!(
                "E-EVENT-007: event `{}` action target `{target}` is not bound in the \
                 simulate inputs map (pass --set {target}=... so switching can rewrite it)",
                event.name
            ));
        }
    } else if state.contains_key(target) {
        state.insert(target.clone(), value);
    } else {
        return Err(format!(
            "E-EVENT-007: event `{}` action target `{target}` is neither a bound input nor \
             a state slot at t={fire_t}",
            event.name
        ));
    }
    Ok(())
}

/// After an event fires, dispatch every transition rule whose `trigger`
/// names that event, in `transitions:` declaration order, applying each
/// rule's actions in rule order. A `state.<name>` action overwrites the
/// state map; a bare action target writes the live input map when the
/// name is bound there and the state map when it is a declared state
/// (mirroring the semantics layer's inputs-then-states lookup; anything
/// else refuses `E-TRANS-007`). The caller owns the single algebraic
/// re-projection after all transitions.
fn apply_transition_actions(
    package: &SemanticPackage,
    declaration: &Declaration,
    live_inputs: &mut BTreeMap<String, Value>,
    state: &mut BTreeMap<String, Value>,
    transitions: &[TransitionDecl],
    fired_event: &EventDecl,
    fire_t: f64,
) -> Result<(), String> {
    // Capture the fired event's parameters at the crossing state: each
    // param binds the live value of the SAME-NAMED variable. Algebraic
    // unknowns bind from the state map (already projected); inputs from
    // the live input map; states from the state map.
    let mut capture: BTreeMap<String, Value> = BTreeMap::new();
    for param in &fired_event.params {
        let value = if declaration.algebraic.iter().any(|field| &field.name == param) {
            state.get(param)
        } else if let Some(value) = live_inputs.get(param) {
            Some(value)
        } else {
            state.get(param)
        };
        let Some(value) = value else {
            return Err(format!(
                "E-TRANS-007: event `{}` parameter `{param}` has no capture value at t={fire_t}",
                fired_event.name
            ));
        };
        capture.insert(param.clone(), value.clone());
    }
    for rule in transitions {
        if rule.trigger != fired_event.name {
            continue;
        }
        for action in &rule.actions {
            let value = eval_event_expr_with_capture(
                package,
                declaration,
                live_inputs,
                state,
                action.expr,
                &capture,
            )?;
            check_finite_action_value(
                "E-TRANS-008",
                &format!("transition on `{}` action", fired_event.name),
                &action.target,
                &value,
            )?;
            if action.is_state {
                if state.contains_key(&action.target) {
                    state.insert(action.target.clone(), value);
                } else {
                    return Err(format!(
                        "E-TRANS-007: transition on `{}` targets non-state `{}` at t={fire_t}",
                        fired_event.name, action.target
                    ));
                }
            } else if live_inputs.contains_key(&action.target) {
                // Bare name bound as an input: write the live input map
                // (inputs win, matching the semantics lookup order; every
                // declared input is bound here because the event
                // condition already evaluated over all of them).
                live_inputs.insert(action.target.clone(), value);
            } else if state.contains_key(&action.target) {
                // Bare name that is a declared STATE: the semantics layer
                // accepts `inputs.or_else(states)` for bare targets, so
                // the bare form lands in the state map exactly like
                // `state.<name>` would.
                state.insert(action.target.clone(), value);
            } else {
                return Err(format!(
                    "E-TRANS-007: transition on `{}` target `{}` is neither a bound input \
                     nor a state slot at t={fire_t}",
                    fired_event.name, action.target
                ));
            }
        }
    }
    Ok(())
}

/// Lane one evaluation of a declared event expression over the model's
/// inputs, algebraic unknowns, and state. Mirrors
/// `eval_definitions_values`' binding order (inputs + algebraic guesses,
/// then states) so `lower_definition` sees identical registers.
fn eval_event_expr(
    package: &SemanticPackage,
    declaration: &Declaration,
    live_inputs: &BTreeMap<String, Value>,
    state: &BTreeMap<String, Value>,
    expr: EmirExprRef,
) -> Result<Value, String> {
    let capture = BTreeMap::new();
    eval_event_expr_with_capture(package, declaration, live_inputs, state, expr, &capture)
}

/// Evaluation lane shared by event conditions/actions and transition
/// actions. `capture` overrides the natural bindings: for each key the
/// captured value (an event parameter bound at the crossing sample) wins
/// over the input/state/ algebraic-derived value of that name.
fn eval_event_expr_with_capture(
    package: &SemanticPackage,
    declaration: &Declaration,
    live_inputs: &BTreeMap<String, Value>,
    state: &BTreeMap<String, Value>,
    expr: EmirExprRef,
    capture: &BTreeMap<String, Value>,
) -> Result<Value, String> {
    let mut bind_names: Vec<String> = declaration
        .inputs
        .iter()
        .map(|field| field.name.clone())
        .collect();
    if let Some(residuals) = package.residuals.get(&declaration.id) {
        if let Some(first) = residuals.first() {
            for name in &first.algebraic {
                if !bind_names.iter().any(|existing| existing == name) {
                    bind_names.push(name.clone());
                }
            }
        }
    }
    let mut bind_values: Vec<Value> = Vec::with_capacity(bind_names.len());
    for name in &bind_names {
        // Algebraic unknowns bind from the STATE map at lane points: the
        // Newton projection has already written their solved values
        // there, so conditions/actions see the consistent manifold.
        let value = if let Some(captured) = capture.get(name) {
            Some(captured.clone())
        } else if declaration.algebraic.iter().any(|field| &field.name == name) {
            state.get(name).cloned()
        } else {
            live_inputs.get(name).cloned()
        };
        let Some(value) = value else {
            return Err(format!(
                "E-EVENT-007: event expression needs `{name}` bound (pass --set {name}=...)"
            ));
        };
        bind_values.push(value);
    }
    let state_names: Vec<String> = declaration
        .state
        .iter()
        .map(|field| field.name.clone())
        .collect();
    // A bare reference to a STATE name (no `state.` prefix) lowers as an
    // input slot. That happens when an event parameter name shadows the
    // state and admission keeps the bare spelling (the parameter is the
    // in-scope binding). Bind those state-named slots from the state map
    // so the condition/action sees the live state value during detection;
    // a captured parameter value still overrides it at firing.
    for field in &declaration.state {
        let name = &field.name;
        if bind_names.iter().any(|existing| existing == name) {
            continue;
        }
        bind_names.push(name.clone());
        bind_values.push(
            capture
                .get(name)
                .cloned()
                .or_else(|| state.get(name).cloned())
                .unwrap_or(Value::F64(f64::NAN)),
        );
    }
    let state_values: Vec<Value> = state_names
        .iter()
        .map(|name| {
            capture
                .get(name)
                .cloned()
                .or_else(|| state.get(name).cloned())
                .unwrap_or(Value::F64(f64::NAN))
        })
        .collect();
    let program = crate::lower_definition(package, expr, &bind_names, &state_names)
        .map_err(|detail| format!("E-EVENT-008: event expression refused: {detail}"))?;
    crate::interp::evaluate(&program, &bind_values, &state_values)
        .map_err(|fault| format!("E-EVENT-008: event expression fault: {fault}"))
}

struct CashKarp {
    fourth: BTreeMap<String, Value>,
    fifth: BTreeMap<String, Value>,
}

fn cash_karp_stages(
    package: &SemanticPackage,
    declaration: &Declaration,
    inputs: &BTreeMap<String, Value>,
    state: &BTreeMap<String, Value>,
    dt: f64,
) -> Result<CashKarp, String> {
    let skip = algebraic_name_set(declaration);
    let k1 = eval_rates(package, declaration, inputs, state)?;
    let s2 = apply_scaled(state, &[(1.0 / 5.0, &k1)], dt, &skip)?;
    let k2 = eval_rates(package, declaration, inputs, &s2)?;
    let s3 = apply_scaled(state, &[(3.0 / 40.0, &k1), (9.0 / 40.0, &k2)], dt, &skip)?;
    let k3 = eval_rates(package, declaration, inputs, &s3)?;
    let s4 = apply_scaled(
        state,
        &[(3.0 / 10.0, &k1), (-9.0 / 10.0, &k2), (6.0 / 5.0, &k3)],
        dt,
        &skip,
    )?;
    let k4 = eval_rates(package, declaration, inputs, &s4)?;
    let s5 = apply_scaled(
        state,
        &[
            (-11.0 / 54.0, &k1),
            (5.0 / 2.0, &k2),
            (-70.0 / 27.0, &k3),
            (35.0 / 27.0, &k4),
        ],
        dt,
        &skip,
    )?;
    let k5 = eval_rates(package, declaration, inputs, &s5)?;
    let s6 = apply_scaled(
        state,
        &[
            (1631.0 / 55296.0, &k1),
            (175.0 / 512.0, &k2),
            (575.0 / 13824.0, &k3),
            (44275.0 / 110592.0, &k4),
            (253.0 / 4096.0, &k5),
        ],
        dt,
        &skip,
    )?;
    let k6 = eval_rates(package, declaration, inputs, &s6)?;
    let fifth = apply_scaled(
        state,
        &[
            (37.0 / 378.0, &k1),
            (250.0 / 621.0, &k3),
            (125.0 / 594.0, &k4),
            (512.0 / 1771.0, &k6),
        ],
        dt,
        &skip,
    )?;
    let fourth = apply_scaled(
        state,
        &[
            (2825.0 / 27648.0, &k1),
            (18575.0 / 48384.0, &k3),
            (13525.0 / 55296.0, &k4),
            (277.0 / 14336.0, &k5),
            (1.0 / 4.0, &k6),
        ],
        dt,
        &skip,
    )?;
    Ok(CashKarp { fourth, fifth })
}

fn adaptive_rk45_try(
    package: &SemanticPackage,
    declaration: &Declaration,
    inputs: &BTreeMap<String, Value>,
    state: &BTreeMap<String, Value>,
    dt: f64,
    options: &SimulateOptions,
) -> Result<(BTreeMap<String, Value>, f64, f64), String> {
    let stages = cash_karp_stages(package, declaration, inputs, state, dt)?;
    // Rust `f64::max` ignores NaN, so a NaN fourth/fifth pair would otherwise
    // report err=0 and be accepted as a perfect step.
    if !values_finite(&stages.fourth) || !values_finite(&stages.fifth) {
        return Err("adaptive RK45 step produced a non-finite state".to_string());
    }
    let err = state_error(&stages.fourth, &stages.fifth);
    let scale = error_scale(state, &stages.fifth, options);
    let rel = if scale > 0.0 { err / scale } else { err };
    if !rel.is_finite() {
        return Err("adaptive RK45 error estimate is non-finite".to_string());
    }
    if rel <= 1.0 {
        Ok((stages.fifth, dt, rel))
    } else {
        let next = (0.9 * dt * rel.powf(-0.2)).max(dt * 0.2);
        if !next.is_finite() || next >= dt {
            return Err("adaptive step rejected but could not shrink dt".to_string());
        }
        Ok((stages.fifth, next, rel))
    }
}

fn grow_step(dt: f64, rel: f64, options: &SimulateOptions, remaining: f64) -> f64 {
    let grown = if rel <= 0.0 {
        dt * 5.0
    } else {
        (0.9 * dt * rel.powf(-0.2)).min(dt * 5.0).max(dt)
    };
    let grown = match options.dt_max {
        Some(dt_max) => grown.min(dt_max),
        None => grown,
    };
    grown.min(remaining.max(dt))
}

fn state_error(left: &BTreeMap<String, Value>, right: &BTreeMap<String, Value>) -> f64 {
    let mut max = 0.0_f64;
    for (name, a) in left {
        if let Some(b) = right.get(name) {
            let diff = value_abs_diff(a, b);
            // `f64::max` returns the non-NaN arg when the other is NaN, which
            // would under-report a poisoned comparison as err=0.
            if !diff.is_finite() {
                return f64::INFINITY;
            }
            max = max.max(diff);
        }
    }
    max
}

fn values_finite(state: &BTreeMap<String, Value>) -> bool {
    state.values().all(value_is_finite)
}

fn value_is_finite(value: &Value) -> bool {
    match value {
        Value::F64(number) => number.is_finite(),
        Value::I64(_) | Value::Bool(_) | Value::Text(_) | Value::Rat { .. } => true,
        Value::Series { points, .. } => points
            .iter()
            .all(|(time, value)| time.is_finite() && value.is_finite()),
        Value::Set(values) => values.iter().all(value_is_finite),
        Value::Record { fields, .. } => fields.values().all(value_is_finite),
        Value::Complex { re, im } => re.is_finite() && im.is_finite(),
        Value::Vector(items) => items.iter().all(|item| item.is_finite()),
        Value::Matrix { data, .. } | Value::Tensor { data, .. } => {
            data.iter().all(|item| item.is_finite())
        }
        Value::Interval { lo, hi } => lo.is_finite() && hi.is_finite(),
        // Option/Result carriers (aj8d): finite iff the payload is
        // (a None carries nothing, trivially finite).
        Value::Option(None) => true,
        Value::Option(Some(inner)) => value_is_finite(inner),
        Value::Result { payload, .. } => value_is_finite(payload),
    }
}

fn error_scale(
    start: &BTreeMap<String, Value>,
    end: &BTreeMap<String, Value>,
    options: &SimulateOptions,
) -> f64 {
    let atol = options.atol.unwrap_or(1e-6);
    let rtol = options.rtol.unwrap_or(1e-3);
    let mut max = atol;
    for (name, a) in start {
        let mag = value_abs_max(a).max(end.get(name).map(value_abs_max).unwrap_or(0.0));
        max = max.max(atol + rtol * mag);
    }
    max
}

fn value_abs_diff(left: &Value, right: &Value) -> f64 {
    match (left, right) {
        (Value::F64(a), Value::F64(b)) => (a - b).abs(),
        (Value::I64(a), Value::I64(b)) => (*a as f64 - *b as f64).abs(),
        (Value::I64(a), Value::F64(b)) => (*a as f64 - b).abs(),
        (Value::F64(a), Value::I64(b)) => (a - *b as f64).abs(),
        (Value::Vector(a), Value::Vector(b)) => a
            .iter()
            .zip(b.iter())
            .map(|(x, y)| (x - y).abs())
            .fold(0.0, f64::max),
        (Value::Matrix { data: a, .. }, Value::Matrix { data: b, .. })
        | (Value::Tensor { data: a, .. }, Value::Tensor { data: b, .. }) => a
            .iter()
            .zip(b.iter())
            .map(|(x, y)| (x - y).abs())
            .fold(0.0, f64::max),
        _ => f64::INFINITY,
    }
}

fn value_abs_max(value: &Value) -> f64 {
    match value {
        Value::F64(number) => number.abs(),
        Value::I64(number) => (*number as f64).abs(),
        Value::Rat { num, den } => {
            let num = num.unsigned_abs() as f64;
            let den = den.unsigned_abs() as f64;
            if den == 0.0 { f64::INFINITY } else { num / den }
        }
        Value::Bool(_) | Value::Text(_) => 0.0,
        Value::Series { points, .. } => points.iter().fold(0.0_f64, |acc, (time, value)| {
            acc.max(time.abs()).max(value.abs())
        }),
        Value::Set(values) => values
            .iter()
            .fold(0.0_f64, |acc, value| acc.max(value_abs_max(value))),
        Value::Record { fields, .. } => fields
            .values()
            .fold(0.0_f64, |acc, value| acc.max(value_abs_max(value))),
        Value::Complex { re, im } => re.hypot(*im),
        Value::Vector(items) => items.iter().fold(0.0, |acc, item| acc.max(item.abs())),
        Value::Matrix { data, .. } | Value::Tensor { data, .. } => {
            data.iter().fold(0.0, |acc, item| acc.max(item.abs()))
        }
        Value::Interval { lo, hi } => lo.abs().max(hi.abs()),
        // Option/Result carriers (aj8d): magnitude of the payload
        // (None contributes 0.0 — nothing to be finite about).
        Value::Option(None) => 0.0,
        Value::Option(Some(inner)) => value_abs_max(inner),
        Value::Result { payload, .. } => value_abs_max(payload),
    }
}

fn locate_event(
    package: &SemanticPackage,
    declaration: &Declaration,
    inputs: &BTreeMap<String, Value>,
    start: &BTreeMap<String, Value>,
    end: &BTreeMap<String, Value>,
    t0: f64,
    dt: f64,
    name: &str,
    target: f64,
    method: StepMethod,
) -> Result<Option<(f64, BTreeMap<String, Value>)>, String> {
    let g0 = event_gap(start, name, target)?;
    let g1 = event_gap(end, name, target)?;
    // Non-finite gaps make the sign test and bisection silent-wrong
    // (NaN comparisons are never > 0, so a blow-up looks like a crossing).
    if !g0.is_finite() || !g1.is_finite() {
        return Err(format!(
            "event state `{name}` produced a non-finite gap (start={g0}, end={g1})"
        ));
    }
    if g0 == 0.0 {
        return Ok(Some((t0, start.clone())));
    }
    if g0 * g1 > 0.0 {
        return Ok(None);
    }
    let mut lo_t = t0;
    let mut hi_t = t0 + dt;
    let mut lo = start.clone();
    let mut hi = end.clone();
    let mut glo = g0;
    for _ in 0..EVENT_LOCATE_ITERATIONS {
        let mid_t = 0.5 * (lo_t + hi_t);
        let mid = step_continuous_values(package, declaration, inputs, &lo, mid_t - lo_t, method)?;
        let gmid = event_gap(&mid, name, target)?;
        if !gmid.is_finite() {
            return Err(format!(
                "event state `{name}` produced a non-finite gap during location (g={gmid})"
            ));
        }
        if gmid == 0.0 || (hi_t - lo_t).abs() <= EVENT_LOCATE_TOLERANCE {
            return Ok(Some((mid_t, mid)));
        }
        if glo.signum() == gmid.signum() {
            lo_t = mid_t;
            lo = mid;
            glo = gmid;
        } else {
            hi_t = mid_t;
            hi = mid;
        }
    }
    Ok(Some((hi_t, hi)))
}

fn event_gap(state: &BTreeMap<String, Value>, name: &str, target: f64) -> Result<f64, String> {
    let Some(value) = state.get(name) else {
        return Err(format!("event state `{name}` is missing"));
    };
    match value {
        Value::F64(number) => Ok(*number - target),
        Value::I64(number) => Ok(*number as f64 - target),
        _ => Err(format!("event state `{name}` must be a scalar")),
    }
}

fn scalar_map_to_values(map: &BTreeMap<String, f64>) -> BTreeMap<String, Value> {
    map.iter()
        .map(|(name, value)| (name.clone(), Value::F64(*value)))
        .collect()
}

fn values_to_scalars(map: &BTreeMap<String, Value>) -> Result<BTreeMap<String, f64>, String> {
    let mut out = BTreeMap::new();
    for (name, value) in map {
        match value {
            Value::F64(number) => {
                out.insert(name.clone(), *number);
            }
            Value::I64(number) => {
                out.insert(name.clone(), *number as f64);
            }
            _ => return Err(format!("state `{name}` is not a scalar")),
        }
    }
    Ok(out)
}

fn algebraic_name_set(declaration: &Declaration) -> BTreeSet<String> {
    declaration
        .algebraic
        .iter()
        .map(|field| field.name.clone())
        .collect()
}

/// Re-solve `algebraic:` unknowns at `state` and write them into the map.
/// After a successful DAE step the returned (differential + algebraic)
/// point sits on the constraint manifold: max |residual| ≤ 1e-6.
fn project_algebraic_into(
    package: &SemanticPackage,
    declaration: &Declaration,
    inputs: &BTreeMap<String, Value>,
    state: &mut BTreeMap<String, Value>,
) -> Result<(), String> {
    if declaration.algebraic.is_empty() {
        return Ok(());
    }
    let residuals = package
        .residuals
        .get(&declaration.id)
        .map(|residuals| residuals.as_slice())
        .unwrap_or(&[]);
    if residuals.is_empty() {
        return Ok(());
    }
    let algebraic_names: Vec<String> = declaration
        .algebraic
        .iter()
        .map(|field| field.name.clone())
        .collect();
    let mut rate_names: Vec<String> = Vec::new();
    for residual in residuals.iter() {
        for rate in &residual.rates {
            if !rate_names.iter().any(|name| name == rate) {
                rate_names.push(rate.clone());
            }
        }
    }
    let mut guess_inputs = inputs.clone();
    for name in &algebraic_names {
        if let Some(value) = state.get(name) {
            guess_inputs.insert(name.clone(), value.clone());
        }
    }
    let (solved, _) = causal_newton(
        package,
        declaration,
        &guess_inputs,
        state,
        &residuals,
        &algebraic_names,
        &rate_names,
    )?;
    for (name, value) in solved {
        state.insert(name, value);
    }
    Ok(())
}

/// One IMPLICIT (backward) Euler step for a scalar-differential-state
/// model (xx0x.3 runner slice): Newton on `r(x) = x − x_n − h·f(x)`
/// using the model's own rate evaluation. The rate may be nonlinear;
/// Newton converges to the machine-exact implicit point or refuses
/// `E-ODE-001`. Non-positive `dt` refuses `E-ODE-003` (the explicit
/// path's rule applies here too: a non-advancing step must never
/// return the input as an "integrated" value). Scalar carrier per the
/// nucleus slice: exactly one differential state.
fn implicit_backward_euler_step(
    package: &SemanticPackage,
    declaration: &Declaration,
    inputs: &BTreeMap<String, Value>,
    state: &BTreeMap<String, Value>,
    dt: f64,
    skip: &BTreeSet<String>,
) -> Result<BTreeMap<String, Value>, String> {
    if !dt.is_finite() || dt <= 0.0 {
        return Err(format!(
            "E-ODE-003: step size must be a positive finite Float64 for backward Euler \
             (non-advancing steps refuse), got {dt}"
        ));
    }
    let differential: Vec<&String> = state.keys().filter(|name| !skip.contains(*name)).collect();
    if differential.len() != 1 {
        return Err(format!(
            "E-ODE-001: backward Euler's nucleus carrier is ONE differential state (scalar \
             stiff ODE); this model has {} (vector/DAE coupling is the follow-up slice)",
            differential.len()
        ));
    }
    let name = differential[0].clone();
    let y0 = match state.get(&name) {
        Some(Value::F64(v)) => *v,
        Some(Value::I64(v)) => *v as f64,
        other => {
            return Err(format!(
                "E-ODE-001: backward Euler's scalar carrier needs a scalar state `{name}`, \
                 found {other:?}"
            ));
        }
    };
    // Implicit Newton on r(y) = y − y0 − h·f(y). The rate evaluation
    // goes through the model's own machinery so definitions and
    // algebraic unknowns participate identically to the explicit path.
    let rate_at = |state_at: &BTreeMap<String, Value>| -> Result<f64, String> {
        let rates = eval_rates(package, declaration, inputs, state_at)?;
        match rates.get(&name) {
            Some(Value::F64(v)) => Ok(*v),
            Some(Value::I64(v)) => Ok(*v as f64),
            other => Err(format!(
                "E-ODE-001: backward Euler's scalar carrier needs a scalar rate \
                 `der_{name}`, found {other:?}"
            )),
        }
    };
    let mut y = y0; // start from the explicit guess — always finite here
    let mut converged = false;
    const MAX_ITER: usize = 50;
    for _ in 0..MAX_ITER {
        let f = rate_at(&state_of_y(&name, y))?;
        let residual = y - y0 - dt * f;
        if residual.abs() < 1e-12 * (1.0 + y.abs()) {
            converged = true;
            break;
        }
        // Forward-difference Jacobian dr/dy ≈ 1 − h·(f(y+h) − f(y))/h.
        let h_fd = 1e-7 * (1.0 + y.abs());
        let f_plus = rate_at(&state_of_y(&name, y + h_fd))?;
        let d = 1.0 - dt * (f_plus - f) / h_fd;
        if !d.is_finite() || d.abs() < 1e-300 {
            return Err(format!(
                "E-ODE-001: implicit residual Jacobian is singular (dr/dy = {d}); the \
                 implicit equation may have no solution for this state and dt"
            ));
        }
        let step = residual / d;
        y -= step;
        if !y.is_finite() {
            return Err(format!(
                "E-ODE-001: Newton iterate left the finite range while solving the implicit \
                 step (residual {residual:.3e}); no real solution reachable"
            ));
        }
    }
    if !converged {
        let residual_check = y - y0 - dt * rate_at(&state_of_y(&name, y))?;
        return Err(format!(
            "E-ODE-001: implicit backward-Euler step did not converge within {MAX_ITER} Newton \
             iterations (residual {residual_check:.3e}); the implicit equation may have no \
             real solution at this state and dt — refine the model or reduce the step"
        ));
    }
    let mut next = state.clone();
    next.insert(name, Value::F64(y));
    // Algebraic unknowns project as on every other path.
    project_algebraic_into(package, declaration, inputs, &mut next)?;
    Ok(next)
}

/// State map with a single scalar entry replaced (`y = value`).
fn state_of_y(name: &str, value: f64) -> BTreeMap<String, Value> {
    let mut map = BTreeMap::new();
    map.insert(name.to_string(), Value::F64(value));
    map
}

/// One velocity-Verlet step for the separable system `q' = v`,
/// `v' = a(q)` (xx0x.3 runner slice): kick-drift-kick with ONE
/// acceleration evaluation pair per step. The STRUCTURE gate refuses
/// `E-ODE-002` unless the model is exactly this shape — `der_q = v`
/// (the identity) and the acceleration independent of `v` — because
/// symplectic integrators preserve structure only for
/// structure-preserving problems. Negative `dt` is legal (time
/// reversal is the law the bead tests).
fn velocity_verlet_step(
    package: &SemanticPackage,
    declaration: &Declaration,
    inputs: &BTreeMap<String, Value>,
    state: &BTreeMap<String, Value>,
    dt: f64,
) -> Result<BTreeMap<String, Value>, String> {
    if !dt.is_finite() || dt == 0.0 {
        return Err(format!(
            "E-ODE-003: step size must be a finite non-zero Float64 for velocity Verlet, \
             got {dt}"
        ));
    }
    let structure_error = |why: &str| -> String {
        format!(
            "E-ODE-002: velocity Verlet needs the SEPARABLE carrier `q' = v, v' = a(q)` \
             (symplectic integrators preserve structure only for structure-preserving \
             problems); this model {why}"
        )
    };
    // Exactly two differential states q, v; q's rate IS v.
    let q_names: Vec<&String> = declaration.state.iter().map(|field| &field.name).collect();
    if q_names.len() != 2 {
        return Err(structure_error(&format!(
            "has {} states; the separable carrier needs exactly (q, v)",
            q_names.len()
        )));
    }
    let q_name = q_names[0].clone();
    let v_name = q_names[1].clone();
    let q0 = scalar_of(state, &q_name)?;
    let v0 = scalar_of(state, &v_name)?;
    // Structure gate 1: q's rate must be EXACTLY the velocity identity
    // (`der_q = v`) — probed at two different v values, because at
    // v = 0 the identity `q' = v` and `q' = 2·v` coincide and a
    // proportional rate would slip through.
    let rates_at_q = eval_rates(package, declaration, inputs, state)?;
    let q_rate = match rates_at_q.get(&q_name) {
        Some(Value::F64(v)) => *v,
        other => return Err(structure_error(&format!("has non-scalar q rate {other:?}"))),
    };
    if (q_rate - v0).abs() > 1e-9 * (1.0 + v0.abs()) {
        return Err(structure_error(
            "does not satisfy `der_q = v` (the separable identity)",
        ));
    }
    let mut v_probed = state.clone();
    v_probed.insert(v_name.clone(), Value::F64(v0 + 1.0));
    let rates_probed = eval_rates(package, declaration, inputs, &v_probed)?;
    let q_rate_probed = match rates_probed.get(&q_name) {
        Some(Value::F64(v)) => *v,
        other => return Err(structure_error(&format!("has non-scalar q rate {other:?}"))),
    };
    if (q_rate_probed - (v0 + 1.0)).abs() > 1e-9 * (2.0 + v0.abs()) {
        return Err(structure_error(
            "does not satisfy `der_q = v` (the separable identity) away from the initial \
             velocity",
        ));
    }
    // Acceleration a(q): the acceleration must be UNCHANGED when the
    // velocity changes — the probe above already evaluated the rates at
    // v0 + 1 (a DIFFERENT velocity; probing only at v = v0 = 0 would let
    // a linear `a(q, v)` slip through), so read the velocity-rate from
    // that same evaluation instead of recomputing it. Any a(q, v)
    // dependence on v is not separable.
    let acceleration_at_v = match rates_probed.get(&v_name) {
        Some(Value::F64(a)) => *a,
        other => return Err(structure_error(&format!("has non-scalar a {other:?}"))),
    };
    // The base acceleration comes from the AT-STATE evaluation
    // (unmodified velocity), not the probe map.
    let acceleration = match rates_at_q.get(&v_name) {
        Some(Value::F64(a)) => *a,
        other => return Err(structure_error(&format!("has non-scalar a {other:?}"))),
    };
    if (acceleration - acceleration_at_v).abs() > 1e-9 * (1.0 + acceleration.abs()) {
        return Err(structure_error(
            "has an acceleration depending on `v` (not separable: `v' = a(q)` required)",
        ));
    }
    // Kick-drift-kick, one acceleration evaluation per half-kick pair:
    // v½ = v + a(q)·h/2; q₁ = q + v½·h; then re-evaluate a(q₁), v₁ =
    // v½ + a(q₁)·h/2.
    let half_kick = v0 + acceleration * dt / 2.0;
    let q1 = q0 + half_kick * dt;
    let mut mid = state.clone();
    mid.insert(q_name.clone(), Value::F64(q1));
    // Re-derive the acceleration at q1 through the model (definitions
    // and algebraic unknowns participate identically to explicit paths).
    let rates_at_q1 = eval_rates(package, declaration, inputs, &mid)?;
    let a1 = match rates_at_q1.get(&v_name) {
        Some(Value::F64(a)) => *a,
        other => return Err(structure_error(&format!("has non-scalar a {other:?}"))),
    };
    let v1 = half_kick + a1 * dt / 2.0;
    let mut next = state.clone();
    next.insert(q_name, Value::F64(q1));
    next.insert(v_name, Value::F64(v1));
    Ok(next)
}

fn scalar_of(state: &BTreeMap<String, Value>, name: &str) -> Result<f64, String> {
    match state.get(name) {
        Some(Value::F64(v)) => Ok(*v),
        Some(Value::I64(v)) => Ok(*v as f64),
        other => Err(format!("state `{name}` must be a scalar, found {other:?}")),
    }
}

fn apply_scaled(
    state: &BTreeMap<String, Value>,
    terms: &[(f64, &BTreeMap<String, Value>)],
    dt: f64,
    skip: &BTreeSet<String>,
) -> Result<BTreeMap<String, Value>, String> {
    let mut next = BTreeMap::new();
    for (name, value) in state {
        if skip.contains(name) {
            // Algebraic keys ride along in the extended DAE state map;
            // they are not integrated and are re-solved after the step.
            continue;
        }
        let mut acc = value.clone();
        for (weight, rates) in terms {
            let rate = rates
                .get(name)
                .ok_or_else(|| format!("missing rate `der_{name}`"))?;
            acc = add_scaled(&acc, rate, dt * *weight)?;
        }
        next.insert(name.clone(), acc);
    }
    Ok(next)
}

fn add_scaled(value: &Value, rate: &Value, scale: f64) -> Result<Value, String> {
    match (value, rate) {
        (Value::F64(x), Value::F64(r)) => Ok(Value::F64(x + scale * r)),
        (Value::I64(x), Value::F64(r)) => Ok(Value::F64(*x as f64 + scale * r)),
        (Value::F64(x), Value::I64(r)) => Ok(Value::F64(x + scale * *r as f64)),
        (Value::I64(x), Value::I64(r)) => Ok(Value::F64(*x as f64 + scale * *r as f64)),
        (Value::Vector(x), Value::Vector(r)) if x.len() == r.len() => Ok(Value::Vector(
            x.iter().zip(r.iter()).map(|(a, b)| a + scale * b).collect(),
        )),
        (
            Value::Matrix {
                rows: r1,
                cols: c1,
                data: d1,
            },
            Value::Matrix {
                rows: r2,
                cols: c2,
                data: d2,
            },
        ) if r1 == r2 && c1 == c2 && d1.len() == d2.len() => Ok(Value::Matrix {
            rows: *r1,
            cols: *c1,
            data: d1
                .iter()
                .zip(d2.iter())
                .map(|(a, b)| a + scale * b)
                .collect(),
        }),
        (
            Value::Tensor {
                shape: s1,
                data: d1,
            },
            Value::Tensor {
                shape: s2,
                data: d2,
            },
        ) if s1 == s2 && d1.len() == d2.len() => Ok(Value::Tensor {
            shape: s1.clone(),
            data: d1
                .iter()
                .zip(d2.iter())
                .map(|(a, b)| a + scale * b)
                .collect(),
        }),
        _ => Err("state and rate must have the same scalar/vector/matrix/tensor shape".to_string()),
    }
}

fn eval_rates(
    package: &SemanticPackage,
    declaration: &Declaration,
    inputs: &BTreeMap<String, Value>,
    state: &BTreeMap<String, Value>,
) -> Result<BTreeMap<String, Value>, String> {
    let residuals = package
        .residuals
        .get(&declaration.id)
        .map(|residuals| residuals.as_slice())
        .unwrap_or(&[]);
    let algebraic_names: Vec<String> = declaration
        .algebraic
        .iter()
        .map(|field| field.name.clone())
        .collect();
    let mut rate_names: Vec<String> = Vec::new();
    for residual in residuals.iter() {
        for rate in &residual.rates {
            if !rate_names.contains(rate) {
                rate_names.push(rate.clone());
            }
        }
    }

    let definitions = super::eval_definitions_values(package, declaration, inputs, state)
        .map_err(|verdict| verdict.reason_text().unwrap_or_else(|| verdict.to_string()))?;

    // Causalized implicit DAEs: solve the residual system with Newton's
    // method at the current state (once per RK stage).
    let (solved_algebraic, solved_rates) = if residuals.is_empty() {
        (BTreeMap::new(), BTreeMap::new())
    } else {
        causal_newton(
            package,
            declaration,
            inputs,
            state,
            &residuals,
            &algebraic_names,
            &rate_names,
        )?
    };

    // Definitions referencing a solved algebraic variable must see the
    // solved value: re-evaluate once with the solution bound.
    let definitions = if solved_algebraic.is_empty() {
        definitions
    } else {
        let mut step_inputs = inputs.clone();
        for (name, value) in &solved_algebraic {
            step_inputs.insert(name.clone(), value.clone());
        }
        super::eval_definitions_values(package, declaration, &step_inputs, state)
            .map_err(|verdict| verdict.reason_text().unwrap_or_else(|| verdict.to_string()))?
    };

    let mut rates = BTreeMap::new();
    for field in &declaration.state {
        let key = format!("der_{}", field.name);
        let value = if rate_names.iter().any(|name| name == &field.name) {
            solved_rates
                .get(&key)
                .cloned()
                .ok_or_else(|| format!("missing solved rate `{key}`"))?
        } else {
            definitions
                .get(&key)
                .cloned()
                .ok_or_else(|| format!("missing rate `{key}`"))?
        };
        match value {
            Value::Bool(_)
            | Value::Text(_)
            | Value::Series { .. }
            | Value::Set(_)
            | Value::Record { .. }
            | Value::Interval { .. } => return Err(format!("rate `{key}` is not numeric")),
            // Option/Result carriers (aj8d) are not numeric rates.
            Value::Option(_) | Value::Result { .. } => {
                return Err(format!("rate `{key}` is not numeric"));
            }
            Value::I64(_)
            | Value::F64(_)
            | Value::Rat { .. }
            | Value::Complex { .. }
            | Value::Vector(_)
            | Value::Matrix { .. }
            | Value::Tensor { .. } => {
                rates.insert(field.name.clone(), value);
            }
        }
    }
    Ok(rates)
}
