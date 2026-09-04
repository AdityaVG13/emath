//! Cash-Karp RK45 adaptive stepping and error control.

use super::*;

pub(super) struct CashKarp {
    pub(super) fourth: BTreeMap<String, Value>,
    pub(super) fifth: BTreeMap<String, Value>,
}

pub(super) fn cash_karp_stages(
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

pub(super) fn adaptive_rk45_try(
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

pub(super) fn grow_step(dt: f64, rel: f64, options: &SimulateOptions, remaining: f64) -> f64 {
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

pub(super) fn state_error(left: &BTreeMap<String, Value>, right: &BTreeMap<String, Value>) -> f64 {
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

pub(super) fn values_finite(state: &BTreeMap<String, Value>) -> bool {
    state.values().all(value_is_finite)
}

pub(super) fn value_is_finite(value: &Value) -> bool {
    match value {
        Value::F64(number) => number.is_finite(),
        Value::I64(_) | Value::Bool(_) | Value::Text(_) | Value::Rat { .. } => true,
        // Stage-2 (emath-t63iz): exact big integers are finite by
        // construction; big codewords carry exact elements only.
        Value::BigInt(_) | Value::BigVector(_) => true,
        Value::Series { points, .. } => points
            .iter()
            .all(|(time, value)| time.is_finite() && value.is_finite()),
        Value::Set(values) => values.iter().all(value_is_finite),
        Value::Record { fields, .. } => fields.values().all(value_is_finite),
        Value::List(values) => values.iter().all(value_is_finite),
        Value::Complex { re, im } => re.is_finite() && im.is_finite(),
        Value::Vector(items) => items.iter().all(|item| item.is_finite()),
        Value::Matrix { data, .. } | Value::Tensor { data, .. } => {
            data.iter().all(|item| item.is_finite())
        }
        Value::Interval { lo, hi } => lo.is_finite() && hi.is_finite(),
        // Option/Result carriers: finite iff the payload is
        // (a None carries nothing, trivially finite).
        Value::Option(None) => true,
        Value::Option(Some(inner)) => value_is_finite(inner),
        Value::Result { payload, .. } => value_is_finite(payload),
        Value::Program(_) => false,
    }
}

pub(super) fn error_scale(
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

pub(super) fn value_abs_diff(left: &Value, right: &Value) -> f64 {
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

pub(super) fn value_abs_max(value: &Value) -> f64 {
    match value {
        Value::F64(number) => number.abs(),
        Value::I64(number) => (*number as f64).abs(),
        Value::Rat { num, den } => {
            let num = num.unsigned_abs() as f64;
            let den = den.unsigned_abs() as f64;
            if den == 0.0 { f64::INFINITY } else { num / den }
        }
        Value::Bool(_) | Value::Text(_) => 0.0,
        // Stage-2 (emath-t63iz): no f64 magnitude is defined for the big
        // lane; INFINITY keeps the RK45 step controller conservative.
        Value::BigInt(_) | Value::BigVector(_) => f64::INFINITY,
        Value::Series { points, .. } => points.iter().fold(0.0_f64, |acc, (time, value)| {
            acc.max(time.abs()).max(value.abs())
        }),
        Value::Set(values) => values
            .iter()
            .fold(0.0_f64, |acc, value| acc.max(value_abs_max(value))),
        Value::Record { fields, .. } => fields
            .values()
            .fold(0.0_f64, |acc, value| acc.max(value_abs_max(value))),
        Value::List(values) => values
            .iter()
            .fold(0.0_f64, |acc, value| acc.max(value_abs_max(value))),
        Value::Complex { re, im } => re.hypot(*im),
        Value::Vector(items) => items.iter().fold(0.0, |acc, item| acc.max(item.abs())),
        Value::Matrix { data, .. } | Value::Tensor { data, .. } => {
            data.iter().fold(0.0, |acc, item| acc.max(item.abs()))
        }
        Value::Interval { lo, hi } => lo.abs().max(hi.abs()),
        // Option/Result carriers: magnitude of the payload
        // (None contributes 0.0 — nothing to be finite about).
        Value::Option(None) => 0.0,
        Value::Option(Some(inner)) => value_abs_max(inner),
        Value::Result { payload, .. } => value_abs_max(payload),
        Value::Program(_) => f64::INFINITY,
    }
}

pub(super) fn locate_event(
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

pub(super) fn event_gap(
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

pub(super) fn scalar_map_to_values(map: &BTreeMap<String, f64>) -> BTreeMap<String, Value> {
    map.iter()
        .map(|(name, value)| (name.clone(), Value::F64(*value)))
        .collect()
}

pub(super) fn values_to_scalars(
    map: &BTreeMap<String, Value>,
) -> Result<BTreeMap<String, f64>, String> {
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
