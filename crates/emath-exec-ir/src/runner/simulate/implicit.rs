//! Implicit DAE stepping (backward Euler + Newton) and
//! velocity-Verlet for second-order systems.

use super::*;

pub(super) fn algebraic_name_set(declaration: &Declaration) -> BTreeSet<String> {
    declaration
        .algebraic
        .iter()
        .map(|field| field.name.clone())
        .collect()
}

/// Re-solve `algebraic:` unknowns at `state` and write them into the map.
/// After a successful DAE step the returned (differential + algebraic)
/// point sits on the constraint manifold: max |residual| ≤ 1e-6.
pub(super) fn project_algebraic_into(
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
/// model (runner slice): Newton on `r(x) = x − x_n − h·f(x)`
/// using the model's own rate evaluation. The rate may be nonlinear;
/// Newton converges to the machine-exact implicit point or refuses
/// `E-ODE-001`. Non-positive `dt` refuses `E-ODE-003` (the explicit
/// path's rule applies here too: a non-advancing step must never
/// return the input as an "integrated" value). Scalar carrier per the
/// nucleus slice: exactly one differential state.
pub(super) fn implicit_backward_euler_step(
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
    let mut index = 0usize;
    let mut max_iter = 0usize;
    let mut residual_at_limit = 0.0;
    loop {
        let f = rate_at(&state_of_y(&name, y))?;
        let check = super::eval_capsule_cell(
            "std.capability.dynamics.model-newton-check",
            vec![Value::F64(y), Value::F64(y0), Value::F64(dt), Value::F64(f)],
        )?;
        let (residual, h_fd, y_plus) = match check {
            Value::Record { fields, .. } => {
                let residual = match fields.get("residual") {
                    Some(Value::F64(residual)) => *residual,
                    _ => {
                        return Err(
                            "model Newton check cell returned a non-float residual".to_string()
                        );
                    }
                };
                let done = match fields.get("converged") {
                    Some(Value::Bool(done)) => *done,
                    _ => {
                        return Err(
                            "model Newton check cell returned a non-boolean verdict".to_string()
                        );
                    }
                };
                let h_fd = match fields.get("h") {
                    Some(Value::F64(h)) => *h,
                    _ => {
                        return Err(
                            "model Newton check cell returned a non-float spacing".to_string()
                        );
                    }
                };
                let budget = match fields.get("budget") {
                    Some(Value::I64(budget)) => *budget,
                    _ => return Err("model Newton budget cell returned a non-integer bound".into()),
                };
                max_iter = usize::try_from(budget).map_err(|_| {
                    "model Newton budget cell returned a non-index bound".to_string()
                })?;
                if index >= max_iter {
                    residual_at_limit = residual;
                    break;
                }
                if done {
                    converged = true;
                    break;
                }
                let y_plus = match fields.get("y_plus") {
                    Some(Value::F64(y_plus)) => *y_plus,
                    _ => {
                        return Err(
                            "model Newton check cell returned a non-float probe".to_string()
                        );
                    }
                };
                (residual, h_fd, y_plus)
            }
            _ => {
                return Err("model Newton check cell did not return a check record".to_string());
            }
        };
        // Forward-difference Jacobian dr/dy through the model probe state.
        let f_plus = rate_at(&state_of_y(&name, y_plus))?;
        let update = super::eval_capsule_cell(
            "std.capability.dynamics.model-newton-update",
            vec![
                Value::F64(y),
                Value::F64(dt),
                Value::F64(f),
                Value::F64(f_plus),
                Value::F64(residual),
                Value::F64(h_fd),
            ],
        )?;
        match update {
            Value::Record { fields, .. } => {
                let d = match fields.get("d") {
                    Some(Value::F64(d)) => *d,
                    _ => {
                        return Err(
                            "model Newton update cell returned a non-float Jacobian".to_string()
                        );
                    }
                };
                let singular = match fields.get("singular") {
                    Some(Value::Bool(singular)) => *singular,
                    _ => {
                        return Err(
                            "model Newton update cell returned a non-boolean verdict".to_string()
                        );
                    }
                };
                if singular {
                    return Err(format!(
                        "E-ODE-001: implicit residual Jacobian is singular (dr/dy = {d}); the \
                         implicit equation may have no solution for this state and dt"
                    ));
                }
                let nonfinite = match fields.get("nonfinite") {
                    Some(Value::Bool(nonfinite)) => *nonfinite,
                    _ => {
                        return Err(
                            "model Newton update cell returned a non-boolean verdict".to_string()
                        );
                    }
                };
                if nonfinite {
                    return Err(format!(
                        "E-ODE-001: Newton iterate left the finite range while solving the implicit \
                         step (residual {residual:.3e}); no real solution reachable"
                    ));
                }
                match fields.get("next") {
                    Some(Value::F64(next)) => y = *next,
                    _ => {
                        return Err(
                            "model Newton update cell returned a non-float iterate".to_string()
                        );
                    }
                }
            }
            _ => {
                return Err("model Newton update cell did not return an update record".to_string());
            }
        }
        index += 1;
    }
    if !converged {
        let residual_check = residual_at_limit;
        return Err(format!(
            "E-ODE-001: implicit backward-Euler step did not converge within {max_iter} Newton \
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
pub(super) fn state_of_y(name: &str, value: f64) -> BTreeMap<String, Value> {
    let mut map = BTreeMap::new();
    map.insert(name.to_string(), Value::F64(value));
    map
}

/// One velocity-Verlet step for the separable system `q' = v`,
/// `v' = a(q)` (runner slice): kick-drift-kick with ONE
/// acceleration evaluation pair per step. The STRUCTURE gate refuses
/// `E-ODE-002` unless the model is exactly this shape — `der_q = v`
/// (the identity) and the acceleration independent of `v` — because
/// symplectic integrators preserve structure only for
/// structure-preserving problems. Negative `dt` is legal (time
/// reversal is the law under test).
pub(super) fn velocity_verlet_step(
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
    if match super::eval_capsule_cell(
        "std.capability.dynamics.model-verlet-reject",
        vec![Value::F64(q_rate), Value::F64(v0), Value::Bool(false)],
    )? {
        Value::Bool(reject) => reject,
        _ => return Err("model Verlet admission cell returned a non-boolean verdict".into()),
    } {
        return Err(structure_error(
            "does not satisfy `der_q = v` (the separable identity)",
        ));
    }
    let mut v_probed = state.clone();
    let probe_velocity = super::eval_capsule_cell(
        "std.capability.dynamics.model-verlet-probe",
        vec![Value::F64(v0)],
    )?;
    if !matches!(probe_velocity, Value::F64(_)) {
        return Err("model Verlet probe cell returned a non-float velocity".into());
    }
    v_probed.insert(v_name.clone(), probe_velocity);
    let rates_probed = eval_rates(package, declaration, inputs, &v_probed)?;
    let q_rate_probed = match rates_probed.get(&q_name) {
        Some(Value::F64(v)) => *v,
        other => return Err(structure_error(&format!("has non-scalar q rate {other:?}"))),
    };
    if match super::eval_capsule_cell(
        "std.capability.dynamics.model-verlet-reject",
        vec![Value::F64(q_rate_probed), Value::F64(v0), Value::Bool(true)],
    )? {
        Value::Bool(reject) => reject,
        _ => return Err("model Verlet admission cell returned a non-boolean verdict".into()),
    } {
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
    if match super::eval_capsule_cell(
        "std.capability.dynamics.model-verlet-reject",
        vec![
            Value::F64(acceleration_at_v),
            Value::F64(acceleration),
            Value::Bool(false),
        ],
    )? {
        Value::Bool(reject) => reject,
        _ => return Err("model Verlet admission cell returned a non-boolean verdict".into()),
    } {
        return Err(structure_error(
            "has an acceleration depending on `v` (not separable: `v' = a(q)` required)",
        ));
    }
    // Kick-drift-kick through the authored half-kick cells, with one
    // acceleration evaluation per half-kick pair: v½ = v + a(q)·h/2;
    // q₁ = q + v½·h; then re-evaluate a(q₁), v₁ = v½ + a(q₁)·h/2.
    let (half_kick, q1) = match super::eval_capsule_cell(
        "std.capability.dynamics.model-verlet-half",
        vec![
            Value::F64(q0),
            Value::F64(v0),
            Value::F64(acceleration),
            Value::F64(dt),
        ],
    )? {
        Value::Record { fields, .. } => {
            let half = match fields.get("half") {
                Some(Value::F64(half)) => *half,
                _ => {
                    return Err("model Verlet half cell returned a non-float kick".to_string());
                }
            };
            let q1 = match fields.get("q") {
                Some(Value::F64(q1)) => *q1,
                _ => {
                    return Err("model Verlet half cell returned a non-float drift".to_string());
                }
            };
            (half, q1)
        }
        _ => {
            return Err("model Verlet half cell did not return a kick record".to_string());
        }
    };
    let mut mid = state.clone();
    mid.insert(q_name.clone(), Value::F64(q1));
    // Re-derive the acceleration at q1 through the model (definitions
    // and algebraic unknowns participate identically to explicit paths).
    let rates_at_q1 = eval_rates(package, declaration, inputs, &mid)?;
    let a1 = match rates_at_q1.get(&v_name) {
        Some(Value::F64(a)) => *a,
        other => return Err(structure_error(&format!("has non-scalar a {other:?}"))),
    };
    let v1 = match super::eval_capsule_cell(
        "std.capability.dynamics.model-verlet-finish",
        vec![Value::F64(half_kick), Value::F64(a1), Value::F64(dt)],
    )? {
        Value::F64(v1) => v1,
        _ => {
            return Err("model Verlet finish cell did not return a float velocity".to_string());
        }
    };
    let mut next = state.clone();
    next.insert(q_name, Value::F64(q1));
    next.insert(v_name, Value::F64(v1));
    Ok(next)
}

pub(super) fn scalar_of(state: &BTreeMap<String, Value>, name: &str) -> Result<f64, String> {
    match state.get(name) {
        Some(Value::F64(v)) => Ok(*v),
        Some(Value::I64(v)) => Ok(*v as f64),
        other => Err(format!("state `{name}` must be a scalar, found {other:?}")),
    }
}

pub(super) fn apply_scaled(
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

pub(super) fn add_scaled(value: &Value, rate: &Value, scale: f64) -> Result<Value, String> {
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

pub(super) fn eval_rates(
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

    let definitions = super::super::eval_definitions_values(package, declaration, inputs, state)
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
        super::super::eval_definitions_values(package, declaration, &step_inputs, state)
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
            // Stage-2 (emath-t63iz): exact big values are not rates.
            | Value::BigInt(_)
            | Value::BigVector(_)
            | Value::Series { .. }
            | Value::Set(_)
            | Value::Record { .. }
            | Value::List(_)
            | Value::Interval { .. }
            | Value::Program(_) | Value::DenseLayout(_) => return Err(format!("rate `{key}` is not numeric")),
            // Option/Result carriers are not numeric rates.
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
