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
    let (fields, s0) = pack_state_fields(state, &skip)?;
    let k1 = eval_rates(package, declaration, inputs, state)?;
    let s2 = combine_step(
        &fields,
        &s0,
        dt,
        "std.capability.dynamics.model-rk45-s2",
        &pack_rate_set(&[&k1], &fields)?,
    )?;
    let k2 = eval_rates(package, declaration, inputs, &s2)?;
    let s3 = combine_step(
        &fields,
        &s0,
        dt,
        "std.capability.dynamics.model-rk45-s3",
        &pack_rate_set(&[&k1, &k2], &fields)?,
    )?;
    let k3 = eval_rates(package, declaration, inputs, &s3)?;
    let s4 = combine_step(
        &fields,
        &s0,
        dt,
        "std.capability.dynamics.model-rk45-s4",
        &pack_rate_set(&[&k1, &k2, &k3], &fields)?,
    )?;
    let k4 = eval_rates(package, declaration, inputs, &s4)?;
    let s5 = combine_step(
        &fields,
        &s0,
        dt,
        "std.capability.dynamics.model-rk45-s5",
        &pack_rate_set(&[&k1, &k2, &k3, &k4], &fields)?,
    )?;
    let k5 = eval_rates(package, declaration, inputs, &s5)?;
    let s6 = combine_step(
        &fields,
        &s0,
        dt,
        "std.capability.dynamics.model-rk45-s6",
        &pack_rate_set(&[&k1, &k2, &k3, &k4, &k5], &fields)?,
    )?;
    let k6 = eval_rates(package, declaration, inputs, &s6)?;
    let fourth = combine_step(
        &fields,
        &s0,
        dt,
        "std.capability.dynamics.model-rk45-fourth",
        &pack_rate_set(&[&k1, &k3, &k4, &k5, &k6], &fields)?,
    )?;
    let fifth = combine_step(
        &fields,
        &s0,
        dt,
        "std.capability.dynamics.model-rk45-fifth",
        &pack_rate_set(&[&k1, &k3, &k4, &k6], &fields)?,
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
    let skip = algebraic_name_set(declaration);
    let (fields, start_flat) = pack_state_fields(state, &skip)?;
    let fourth_flat = flatten_map(&stages.fourth, &fields)?;
    let fifth_flat = flatten_map(&stages.fifth, &fields)?;
    let mut offsets = vec![0.0];
    let mut total = 0.0;
    for (_, layout) in &fields {
        total += layout.len() as f64;
        offsets.push(total);
    }
    let atol = options.atol.unwrap_or(1e-6);
    let rtol = options.rtol.unwrap_or(1e-3);
    let norms = super::eval_capsule_cell(
        "std.capability.dynamics.model-rk45-err-scale",
        vec![
            Value::Vector(fourth_flat),
            Value::Vector(fifth_flat),
            Value::Vector(start_flat),
            Value::Vector(offsets),
            Value::F64(atol),
            Value::F64(rtol),
        ],
    )?;
    let (err, mut scale) = match norms {
        Value::Record { fields, .. } => {
            let err = match fields.get("err") {
                Some(Value::F64(err)) => *err,
                _ => {
                    return Err("model RK45 error cell returned a non-float error".to_string());
                }
            };
            let scale = match fields.get("scale") {
                Some(Value::F64(scale)) => *scale,
                _ => {
                    return Err("model RK45 error cell returned a non-float scale".to_string());
                }
            };
            (err, scale)
        }
        _ => {
            return Err("model RK45 error cell did not return a norm record".to_string());
        }
    };
    // Algebraic fields never pack; fold their tolerance terms with the
    // generic magnitude traversal, recombining by max exactly as the
    // native whole-map fold does.
    for (name, value) in state {
        if skip.contains(name) {
            scale = match super::eval_capsule_cell(
                "std.capability.dynamics.model-algebraic-scale",
                vec![
                    Value::F64(scale),
                    Value::F64(atol),
                    Value::F64(rtol),
                    Value::F64(value_abs_max(value)),
                ],
            )? {
                Value::F64(value) => value,
                _ => return Err("model algebraic tolerance cell returned a non-float scale".into()),
            };
        }
    }
    let decision = super::eval_capsule_cell(
        "std.capability.dynamics.model-rk45-decide",
        vec![Value::F64(err), Value::F64(scale), Value::F64(dt)],
    )?;
    let (code, rel, next) = match decision {
        Value::Record { fields, .. } => {
            let code = match fields.get("code") {
                Some(Value::I64(code)) => *code,
                _ => {
                    return Err("model RK45 decision cell returned a non-integer code".to_string());
                }
            };
            let rel = match fields.get("rel") {
                Some(Value::F64(rel)) => *rel,
                _ => {
                    return Err("model RK45 decision cell returned a non-float ratio".to_string());
                }
            };
            let next = match fields.get("next") {
                Some(Value::F64(next)) => *next,
                _ => {
                    return Err("model RK45 decision cell returned a non-float step".to_string());
                }
            };
            (code, rel, next)
        }
        _ => {
            return Err("model RK45 decision cell did not return a decision record".to_string());
        }
    };
    match code {
        0 => Ok((stages.fifth, dt, rel)),
        1 => Ok((stages.fifth, next, rel)),
        2 => Err("adaptive RK45 error estimate is non-finite".to_string()),
        _ => Err("adaptive step rejected but could not shrink dt".to_string()),
    }
}

pub(super) fn grow_step(
    dt: f64,
    rel: f64,
    options: &SimulateOptions,
    remaining: f64,
) -> Result<f64, String> {
    let (limit, capped) = match options.dt_max {
        Some(dt_max) => (dt_max, true),
        None => (0.0, false),
    };
    match super::eval_capsule_cell(
        "std.capability.dynamics.model-rk45-grow",
        vec![
            Value::F64(dt),
            Value::F64(rel),
            Value::F64(limit),
            Value::Bool(capped),
            Value::F64(remaining),
        ],
    )? {
        Value::F64(grown) => Ok(grown),
        _ => Err("model RK45 growth cell did not return Float64 storage".to_string()),
    }
}

/// One packed differential field: its map name, dense layout, and the
/// flat values packed in `BTreeMap` order. Algebraic (`skip`) fields never
/// pack; they ride along outside the integration and are re-solved after.
type PackedFields = Vec<(String, emath_rt::DenseLayout)>;

/// Flatten one numeric value exactly as the native update formula widens
/// it: `I64` becomes `f64`, vectors/matrices/tensors contribute row-major
/// data. Anything else is not a combinable carrier.
fn flatten_like(value: &Value, layout: &emath_rt::DenseLayout) -> Result<Vec<f64>, String> {
    use emath_rt::DenseLayout;
    let shape_err =
        || "state and rate must have the same scalar/vector/matrix/tensor shape".to_string();
    match (layout, value) {
        (DenseLayout::Scalar, Value::F64(x)) => Ok(vec![*x]),
        (DenseLayout::Scalar, Value::I64(x)) => Ok(vec![*x as f64]),
        (DenseLayout::Vector(n), Value::Vector(items)) if n == &items.len() => Ok(items.clone()),
        (
            DenseLayout::Matrix { rows, cols, len },
            Value::Matrix {
                rows: actual_rows,
                cols: actual_cols,
                data,
            },
        ) if rows == actual_rows && cols == actual_cols && len == &data.len() => Ok(data.clone()),
        (
            DenseLayout::Tensor { shape, len },
            Value::Tensor {
                shape: actual,
                data,
            },
        ) if shape == actual && len == &data.len() => Ok(data.clone()),
        _ => Err(shape_err()),
    }
}

/// Pack every differential state field into one flat vector, recording
/// layouts for the matching unpack. Exotic carriers refuse with the
/// native shape error; every packed state is combined, so no guard
/// precedence is lost against the per-field native loop.
fn pack_state_fields(
    state: &BTreeMap<String, Value>,
    skip: &BTreeSet<String>,
) -> Result<(PackedFields, Vec<f64>), String> {
    let mut fields = Vec::new();
    let mut flat = Vec::new();
    for (name, value) in state {
        if skip.contains(name) {
            continue;
        }
        let Some(layout) = value.dense_layout() else {
            return Err(
                "state and rate must have the same scalar/vector/matrix/tensor shape".to_string(),
            );
        };
        flat.extend(flatten_like(value, &layout)?);
        fields.push((name.clone(), layout));
    }
    Ok((fields, flat))
}

/// Pack rate maps against packed state fields in field-major order, so
/// the native per-field guard precedence holds: a missing rate reports
/// before a later pair shape, exactly as the staged native loop does.
fn pack_rate_set(
    rate_maps: &[&BTreeMap<String, Value>],
    fields: &PackedFields,
) -> Result<Vec<Vec<f64>>, String> {
    let mut flats = vec![Vec::new(); rate_maps.len()];
    for (name, layout) in fields {
        for (index, rates) in rate_maps.iter().enumerate() {
            let rate = rates
                .get(name)
                .ok_or_else(|| format!("missing rate `der_{name}`"))?;
            flats[index].extend(flatten_like(rate, layout)?);
        }
    }
    Ok(flats)
}

/// Flatten an already-packed-shaped map against recorded layouts.
/// Packed outputs always match their fields; anything else names the
/// storage fault instead of inventing storage.
fn flatten_map(map: &BTreeMap<String, Value>, fields: &PackedFields) -> Result<Vec<f64>, String> {
    let mut flat = Vec::new();
    for (name, layout) in fields {
        let value = map
            .get(name)
            .ok_or_else(|| "model step storage does not match state".to_string())?;
        flat.extend(flatten_like(value, layout)?);
    }
    Ok(flat)
}

/// Rebuild the state map from flat update storage using the recorded
/// layouts. Scalars always come back `F64`: the native update widens
/// integer carriers to `F64` on combination.
fn unpack_fields(fields: &PackedFields, data: Vec<f64>) -> Result<BTreeMap<String, Value>, String> {
    let total: usize = fields.iter().map(|(_, layout)| layout.len()).sum();
    if data.len() != total {
        return Err("model step storage does not match state".to_string());
    }
    let mut next = BTreeMap::new();
    let mut offset = 0usize;
    for (name, layout) in fields {
        let end = offset
            .checked_add(layout.len())
            .ok_or_else(|| "model state length exceeds usize".to_string())?;
        let chunk = data
            .get(offset..end)
            .ok_or_else(|| "model step storage does not match state".to_string())?;
        let value = match layout {
            emath_rt::DenseLayout::Scalar => Value::F64(chunk[0]),
            emath_rt::DenseLayout::Vector(_) => Value::Vector(chunk.to_vec()),
            emath_rt::DenseLayout::Matrix { rows, cols, .. } => Value::Matrix {
                rows: *rows,
                cols: *cols,
                data: chunk.to_vec(),
            },
            emath_rt::DenseLayout::Tensor { shape, .. } => Value::Tensor {
                shape: shape.clone(),
                data: chunk.to_vec(),
            },
        };
        next.insert(name.clone(), value);
        offset = end;
    }
    Ok(next)
}

/// One authored Cash-Karp combination over packed flat vectors: the
/// stage coefficients and left-to-right accumulation live in the
/// capsule cell; packing, guards, and map restoration stay here.
fn combine_step(
    fields: &PackedFields,
    state_flat: &[f64],
    dt: f64,
    capability: &str,
    rates: &[Vec<f64>],
) -> Result<BTreeMap<String, Value>, String> {
    let mut args = Vec::with_capacity(2 + rates.len());
    args.push(Value::Vector(state_flat.to_vec()));
    for flat in rates {
        args.push(Value::Vector(flat.clone()));
    }
    args.push(Value::F64(dt));
    let out = super::eval_capsule_cell(capability, args)?;
    let Value::Vector(data) = out else {
        return Err("model step did not return Float64 storage".to_string());
    };
    unpack_fields(fields, data)
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
        Value::Program(_) | Value::DenseLayout(_) => false,
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
        Value::Program(_) | Value::DenseLayout(_) => f64::INFINITY,
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
