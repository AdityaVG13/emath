//! Public simulation entry points.

use super::*;

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

/// `simulate_continuous_with` plus the disposition record: the
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
pub(super) fn classify_projection_failure(
    error: &str,
    constraint_unknowns: &[String],
) -> Continuation {
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
pub(super) fn disposition_refusal(
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
