//! ODE simulation and integration machinery: explicit steppers
//! (Euler / RK4 / Cash-Karp RK45), adaptive dt, event location, and
//! causalized implicit-DAE Newton solving.

use crate::interp::{evaluate, Value};
use crate::{lower_definition, EmirProgram};
use emath_ir::{Declaration, ModelResidual, SemanticPackage};
use std::collections::BTreeMap;

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
    if !dt.is_finite() || dt <= 0.0 {
        return Err("step size must be a positive finite Float64".to_string());
    }
    match method {
        StepMethod::Euler => {
            let rates = eval_rates(package, declaration, inputs, state)?;
            apply_scaled(state, &[(1.0, &rates)], dt)
        }
        StepMethod::Rk4 => {
            let k1 = eval_rates(package, declaration, inputs, state)?;
            let s2 = apply_scaled(state, &[(1.0, &k1)], dt / 2.0)?;
            let k2 = eval_rates(package, declaration, inputs, &s2)?;
            let s3 = apply_scaled(state, &[(1.0, &k2)], dt / 2.0)?;
            let k3 = eval_rates(package, declaration, inputs, &s3)?;
            let s4 = apply_scaled(state, &[(1.0, &k3)], dt)?;
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
            )
        }
        StepMethod::Rk45 => {
            let stages = cash_karp_stages(package, declaration, inputs, state, dt)?;
            Ok(stages.fifth)
        }
    }
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
    let mut samples = vec![TrajectorySample {
        t: t0,
        state: state.clone(),
    }];
    let mut current = state.clone();
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
            adaptive_rk45_try(package, declaration, inputs, &current, step, options)?
        } else {
            let next = step_continuous_values(package, declaration, inputs, &current, step, method)?;
            (next, step, 0.0)
        };
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
        if let Some((name, value)) = &options.event {
            if let Some((event_t, event_state)) = locate_event(
                package,
                declaration,
                inputs,
                &current,
                &next,
                t,
                used,
                name,
                *value,
                method,
            )?
            {
                samples.push(TrajectorySample {
                    t: event_t,
                    state: event_state,
                });
                return Ok(Trajectory {
                    method,
                    dt: h,
                    samples,
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
    })
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
    let k1 = eval_rates(package, declaration, inputs, state)?;
    let s2 = apply_scaled(state, &[(1.0 / 5.0, &k1)], dt)?;
    let k2 = eval_rates(package, declaration, inputs, &s2)?;
    let s3 = apply_scaled(state, &[(3.0 / 40.0, &k1), (9.0 / 40.0, &k2)], dt)?;
    let k3 = eval_rates(package, declaration, inputs, &s3)?;
    let s4 = apply_scaled(
        state,
        &[
            (3.0 / 10.0, &k1),
            (-9.0 / 10.0, &k2),
            (6.0 / 5.0, &k3),
        ],
        dt,
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
        Value::I64(_) | Value::Bool(_) => true,
        Value::Vector(items) => items.iter().all(|item| item.is_finite()),
        Value::Matrix { data, .. } | Value::Tensor { data, .. } => {
            data.iter().all(|item| item.is_finite())
        }
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
        (
            Value::Matrix { data: a, .. },
            Value::Matrix { data: b, .. },
        )
        | (
            Value::Tensor { data: a, .. },
            Value::Tensor { data: b, .. },
        ) => a
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
        Value::Bool(_) => 0.0,
        Value::Vector(items) => items.iter().fold(0.0, |acc, item| acc.max(item.abs())),
        Value::Matrix { data, .. } | Value::Tensor { data, .. } => {
            data.iter().fold(0.0, |acc, item| acc.max(item.abs()))
        }
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
    for _ in 0..40 {
        let mid_t = 0.5 * (lo_t + hi_t);
        let mid = step_continuous_values(package, declaration, inputs, &lo, mid_t - lo_t, method)?;
        let gmid = event_gap(&mid, name, target)?;
        if !gmid.is_finite() {
            return Err(format!(
                "event state `{name}` produced a non-finite gap during location (g={gmid})"
            ));
        }
        if gmid == 0.0 || (hi_t - lo_t).abs() <= 1e-12 {
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

fn event_gap(
    state: &BTreeMap<String, Value>,
    name: &str,
    target: f64,
) -> Result<f64, String> {
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

fn apply_scaled(
    state: &BTreeMap<String, Value>,
    terms: &[(f64, &BTreeMap<String, Value>)],
    dt: f64,
) -> Result<BTreeMap<String, Value>, String> {
    let mut next = BTreeMap::new();
    for (name, value) in state {
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
            x.iter()
                .zip(r.iter())
                .map(|(a, b)| a + scale * b)
                .collect(),
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
        .cloned()
        .unwrap_or_default();
    let algebraic_names: Vec<String> = residuals
        .first()
        .map(|residual| residual.algebraic.clone())
        .unwrap_or_default();
    let mut rate_names: Vec<String> = Vec::new();
    for residual in &residuals {
        for rate in &residual.rates {
            if !rate_names.contains(rate) {
                rate_names.push(rate.clone());
            }
        }
    }

    let definitions = super::eval_definitions_values(package, declaration, inputs, state).map_err(
        |verdict| {
            verdict
                .reason_text()
                .unwrap_or_else(|| verdict.to_string())
        },
    )?;

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
        super::eval_definitions_values(package, declaration, &step_inputs, state).map_err(|verdict| {
            verdict
                .reason_text()
                .unwrap_or_else(|| verdict.to_string())
        })?
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
            Value::Bool(_) => return Err(format!("rate `{key}` is not numeric")),
            Value::I64(_)
            | Value::F64(_)
            | Value::Vector(_)
            | Value::Matrix { .. }
            | Value::Tensor { .. } => {
                rates.insert(field.name.clone(), value);
            }
        }
    }
    Ok(rates)
}

/// One unknown of the causalized residual system.
#[derive(Clone, Copy, Debug)]
struct NewtonUnknown {
    /// Bind-table slot (`LoadInput` index) of the unknown's value.
    bind_index: usize,
    /// Component count (1 for a scalar, `n` for a vector).
    width: usize,
}

/// Solve a model's implicit residual system at the current state.
///
/// Causalization: every equation that is not an explicit rate or an
/// algebraic definition is a residual `left - right`. The unknowns are the
/// declaration's `algebraic:` variables (guesses from `inputs`) plus the
/// implicit state rates `der(x)`. Newton's method iterates
/// `x -= J⁻¹ F(x)`; the Jacobian comes from forward differences (one
/// residual evaluation per unknown component), so residuals may mix any
/// builtin function and vector shapes.
///
/// Returns the solved algebraic values (fed back into definitions) and
/// the solved rate values keyed as `der_<state>`.
fn causal_newton(
    package: &SemanticPackage,
    declaration: &Declaration,
    inputs: &BTreeMap<String, Value>,
    state: &BTreeMap<String, Value>,
    residuals: &[ModelResidual],
    algebraic_names: &[String],
    rate_names: &[String],
) -> Result<(BTreeMap<String, Value>, BTreeMap<String, Value>), String> {
    const MAX_ITER: usize = 30;
    const TOL: f64 = 1e-9;

    let input_names: Vec<String> = declaration
        .inputs
        .iter()
        .map(|field| field.name.clone())
        .collect();
    let state_names: Vec<String> = declaration
        .state
        .iter()
        .map(|field| field.name.clone())
        .collect();
    let mut state_values = Vec::with_capacity(state_names.len());
    for name in &state_names {
        let Some(value) = state.get(name) else {
            return Err(format!("missing state `{name}`"));
        };
        state_values.push(value.clone());
    }

    let mut bind_names = input_names.clone();
    for name in algebraic_names {
        if !bind_names.iter().any(|existing| existing == name) {
            bind_names.push(name.clone());
        }
    }
    let rate_offset = bind_names.len();
    for rate in rate_names {
        bind_names.push(format!("__rate_{rate}"));
    }

    // Declaration inputs must be present — silent `0.0` defaults invent
    // parameter values and can converge to a wrong DAE solution (same
    // refuse-silent-defaults rule as Optimize in interp). Rate unknowns
    // (`__rate_*`) start at 0.0 by construction below.
    let mut bind_values: Vec<Value> = Vec::with_capacity(bind_names.len());
    for name in &bind_names {
        if let Some(value) = inputs.get(name) {
            bind_values.push(value.clone());
            continue;
        }
        if name.starts_with("__rate_") {
            bind_values.push(Value::F64(0.0));
            continue;
        }
        return Err(format!("missing input `{name}`"));
    }

    let mut unknowns: Vec<NewtonUnknown> = Vec::new();
    let mut x: Vec<f64> = Vec::new();
    for (index, name) in algebraic_names.iter().enumerate() {
        let value = inputs
            .get(name)
            .cloned()
            .ok_or_else(|| format!("missing algebraic-variable guess `{name}` in the simulate inputs map"))?;
        let width = value_width(&value, name)?;
        unknowns.push(NewtonUnknown {
            bind_index: input_names.len() + index,
            width,
        });
        append_flatten(&mut x, &value)?;
    }
    for (index, rate) in rate_names.iter().enumerate() {
        let width = match state.get(rate) {
            Some(Value::F64(_)) => 1,
            Some(Value::Vector(items)) => items.len(),
            other => {
                return Err(format!(
                    "rate unknown `der({rate})` needs a scalar or vector state, found {other:?}"
                ));
            }
        };
        let start = value_of_width(width)?;
        unknowns.push(NewtonUnknown {
            bind_index: rate_offset + index,
            width,
        });
        append_flatten(&mut x, &start)?;
        bind_values[rate_offset + index] = start;
    }

    let programs: Vec<EmirProgram> = residuals
        .iter()
        .map(|residual| {
            lower_definition(package, residual.expr, &bind_names, &state_names)
                .map_err(|detail| format!("residual lowering failed: {detail}"))
        })
        .collect::<Result<_, _>>()?;

    let mut f = eval_residuals(&programs, &bind_values, &state_values)?;
    let total = x.len();
    let mut converged = max_abs(&f) < TOL;
    for _ in 0..MAX_ITER {
        if converged {
            break;
        }
        // Forward-difference Jacobian.
        let mut jac = vec![vec![0.0; total]; f.len()];
        let mut offset = 0_usize;
        for unknown in &unknowns {
            for component in 0..unknown.width {
                let column = offset + component;
                let h = 1e-7 * (1.0 + x[column].abs());
                let saved = x[column];
                x[column] += h;
                set_unknowns(&mut bind_values, &unknowns, &x);
                let plus = eval_residuals(&programs, &bind_values, &state_values)?;
                for (i, row) in jac.iter_mut().enumerate() {
                    row[column] = (plus[i] - f[i]) / h;
                }
                x[column] = saved;
            }
            offset += unknown.width;
        }
        set_unknowns(&mut bind_values, &unknowns, &x);

        let delta = gaussian_solve(&jac, &f).map_err(|message| {
            format!("implicit residual Jacobian is singular ({message}); check that the residual equations are independent")
        })?;
        for (column, step) in delta.iter().enumerate() {
            x[column] -= step;
        }
        set_unknowns(&mut bind_values, &unknowns, &x);
        f = eval_residuals(&programs, &bind_values, &state_values)?;
        let scale = x.iter().fold(1.0_f64, |acc, value| acc.max(value.abs()));
        converged = max_abs(&f) < TOL || max_abs(&delta) < 1e-12 * (1.0 + scale);
    }
    if max_abs(&f) > 1e-6 {
        return Err(format!(
            "implicit residual system did not converge within {MAX_ITER} Newton iterations (max residual {:.3e}); check the model equations and `algebraic:` guesses",
            max_abs(&f)
        ));
    }

    let mut algebraic_solved = BTreeMap::new();
    for (index, name) in algebraic_names.iter().enumerate() {
        algebraic_solved.insert(name.clone(), bind_values[input_names.len() + index].clone());
    }
    let mut rate_solved = BTreeMap::new();
    for (index, rate) in rate_names.iter().enumerate() {
        rate_solved.insert(format!("der_{rate}"), bind_values[rate_offset + index].clone());
    }
    Ok((algebraic_solved, rate_solved))
}

fn value_width(value: &Value, name: &str) -> Result<usize, String> {
    match value {
        Value::F64(_) | Value::I64(_) => Ok(1),
        Value::Vector(items) => Ok(items.len()),
        _ => Err(format!(
            "algebraic variable `{name}` must be a scalar or vector, found {value:?}"
        )),
    }
}

fn value_of_width(width: usize) -> Result<Value, String> {
    if width == 1 {
        Ok(Value::F64(0.0))
    } else {
        Ok(Value::Vector(vec![0.0; width]))
    }
}

fn append_flatten(out: &mut Vec<f64>, value: &Value) -> Result<(), String> {
    match value {
        Value::F64(v) => out.push(*v),
        Value::I64(v) => out.push(*v as f64),
        Value::Vector(items) => out.extend_from_slice(items),
        _ => return Err("unknown must be a scalar or vector".to_string()),
    }
    Ok(())
}

fn set_unknowns(bind_values: &mut [Value], unknowns: &[NewtonUnknown], x: &[f64]) {
    let mut offset = 0_usize;
    for unknown in unknowns {
        bind_values[unknown.bind_index] = if unknown.width == 1 {
            Value::F64(x[offset])
        } else {
            Value::Vector(x[offset..offset + unknown.width].to_vec())
        };
        offset += unknown.width;
    }
}

fn eval_residuals(
    programs: &[EmirProgram],
    bind_values: &[Value],
    state_values: &[Value],
) -> Result<Vec<f64>, String> {
    let mut out = Vec::new();
    for program in programs {
        let value =
            evaluate(program, bind_values, state_values).map_err(|fault| {
                format!("residual evaluation fault: {fault:?}")
            })?;
        match value {
            Value::F64(v) => out.push(v),
            Value::I64(v) => out.push(v as f64),
            Value::Vector(items) => out.extend_from_slice(&items),
            other => {
                return Err(format!(
                    "residual must evaluate to a scalar or vector, found {other:?}"
                ));
            }
        }
    }
    Ok(out)
}

fn max_abs(values: &[f64]) -> f64 {
    values.iter().fold(0.0_f64, |acc, value| acc.max(value.abs()))
}

/// Solve `matrix * x = rhs` by Gaussian elimination with partial pivoting.
fn gaussian_solve(matrix: &[Vec<f64>], rhs: &[f64]) -> Result<Vec<f64>, String> {
    let n = rhs.len();
    if matrix.len() != n || matrix.iter().any(|row| row.len() != n) {
        return Err("Jacobian is not square".to_string());
    }
    if n == 0 {
        return Ok(Vec::new());
    }
    let mut a: Vec<Vec<f64>> = matrix.to_vec();
    let mut b: Vec<f64> = rhs.to_vec();
    for col in 0..n {
        let mut pivot = col;
        let mut best = a[col][col].abs();
        for row in (col + 1)..n {
            let candidate = a[row][col].abs();
            if candidate > best {
                best = candidate;
                pivot = row;
            }
        }
        if best < 1e-300 {
            return Err(format!("near-zero pivot in column {col}"));
        }
        a.swap(col, pivot);
        b.swap(col, pivot);
        for row in (col + 1)..n {
            let factor = a[row][col] / a[col][col];
            if factor == 0.0 {
                continue;
            }
            for k in col..n {
                a[row][k] -= factor * a[col][k];
            }
            b[row] -= factor * b[col];
        }
    }
    let mut x = vec![0.0; n];
    for row in (0..n).rev() {
        let mut acc = b[row];
        for k in (row + 1)..n {
            acc -= a[row][k] * x[k];
        }
        x[row] = acc / a[row][row];
    }
    Ok(x)
}
