use super::{Path, CliExit, constructor_inspect, SavedRun, successor, diagnostic, EXIT_USAGE, emit, EXIT_OK, resume_package, jobs, EXIT_REFUSED, execute_case, JsonWriter, TypeNode, Value};

pub(crate) fn inspect(path: &Path, json: bool) -> CliExit {
    if let Some(exit) = constructor_inspect(path, json) {
        return exit;
    }
    match SavedRun::load(path) {
        Ok(mut state) => {
            let mut path = path.to_path_buf();
            loop {
                match successor(&state, &path) {
                    Ok(Some((saved, destination))) => {
                        state = saved;
                        path = destination;
                    }
                    Ok(None) => break,
                    Err(error) => return diagnostic(json, EXIT_USAGE, "E-RUN-STATE", &error),
                }
            }
            emit(&state, &path, json, None);
            EXIT_OK
        }
        Err(error) => diagnostic(json, EXIT_USAGE, "E-RUN-STATE", &error),
    }
}

pub(crate) fn verify(path: &Path, json: bool) -> CliExit {
    let state = match SavedRun::load(path) {
        Ok(state) => state,
        Err(error) => return diagnostic(json, EXIT_USAGE, "E-RUN-STATE", &error),
    };
    let package = match resume_package(&state, json) {
        Ok(package) => package,
        Err(exit) => return exit,
    };
    let jobs = match jobs(&package, &state) {
        Ok(jobs) if jobs.len() == state.total => jobs,
        _ => {
            return diagnostic(
                json,
                EXIT_REFUSED,
                "E-RUN-STATE",
                "saved case plan differs from the admitted source",
            );
        }
    };
    let checked_cases = state.completed.len() + usize::from(state.active_result.is_some());
    for (index, &(declaration, test)) in jobs.iter().take(checked_cases).enumerate() {
        let frames = state.methods.get(&index).map(Vec::as_slice).unwrap_or(&[]);
        let result = execute_case(
            &package,
            &package.declarations[declaration],
            test,
            &state.given,
            frames,
            false,
        );
        let expected = state.completed.get(index).or(state.active_result.as_ref());
        if !result.as_ref().is_ok_and(|(result, observed)| Some(result) == expected && observed == frames) {
            return diagnostic(
                json,
                EXIT_REFUSED,
                "E-RUN-VERIFY",
                &format!("certificate checking or source reconstruction disagrees with saved case {index}"),
            );
        }
    }
    if json {
        let mut out = JsonWriter::object();
        out.string("schema", "emath.run-verification.v1");
        out.bool("verified", true);
        out.string("target_id", &state.target_id());
        out.bool("original_target_preserved", state.preserves_original());
        if state.measure > 0 {
            out.bool("measurements_verified", false);
        }
        out.string(
            "scope",
            "authored certificate checks and source result reconstruction; no refinement replay, measurement proof, or execution-history claim",
        );
        out.int("checked_cases", checked_cases as u64);
        out.int("checked_methods", state.methods.values().map(Vec::len).sum::<usize>() as u64);
        println!("{}", out.finish());
    } else {
        println!(
            "verified {checked_cases} cases by certificate checks and source reconstruction; not a formal proof"
        );
    }
    EXIT_OK
}

/// Shared CLI literal parser. Exact integer inputs never pass through f64.
pub fn parse_set_value_for(declared: Option<&TypeNode>, raw: &str) -> Option<Value> {
    let value = match declared {
        Some(TypeNode::BigInt) => Value::parse_bigint(raw),
        Some(TypeNode::Int) => raw.trim().parse::<i64>().ok().map(Value::I64),
        Some(TypeNode::Nat) => raw
            .trim()
            .parse::<i64>()
            .ok()
            .filter(|value| *value >= 0)
            .map(Value::I64),
        Some(TypeNode::Bool) => raw.trim().parse::<bool>().ok().map(Value::Bool),
        Some(TypeNode::Vector { element, .. })
            if matches!(&**element, TypeNode::Int | TypeNode::Nat) =>
        {
            let inner = raw.trim().strip_prefix('[')?.strip_suffix(']')?;
            let values: Option<Vec<f64>> = inner
                .split(',')
                .map(|part| {
                    let value = part.trim().parse::<i64>().ok()?;
                    if !(-9_007_199_254_740_992..=9_007_199_254_740_992).contains(&value)
                        || (matches!(&**element, TypeNode::Nat) && value < 0)
                    {
                        return None;
                    }
                    Some(value as f64)
                })
                .collect();
            values
                .filter(|values| !values.is_empty())
                .map(Value::Vector)
        }
        _ => parse_set_value(raw),
    }?;
    declared
        .is_none_or(|ty| binding_matches(ty, &value))
        .then_some(value)
}

/// Existing finite scalar/vector CLI vocabulary; source examples retain
/// the language's complete literal and expression vocabulary.
pub fn parse_set_value(raw: &str) -> Option<Value> {
    let raw = raw.trim();
    if let Some(inner) = raw
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
    {
        let values: Option<Vec<f64>> = inner
            .split(',')
            .map(|part| {
                part.trim()
                    .parse::<f64>()
                    .ok()
                    .filter(|value| value.is_finite())
            })
            .collect();
        return values
            .filter(|values| !values.is_empty())
            .map(Value::Vector);
    }
    raw.parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
        .map(Value::F64)
}

pub(super) fn binding_matches(ty: &TypeNode, value: &Value) -> bool {
    match (ty, value) {
        (TypeNode::Float64, Value::F64(_))
        | (TypeNode::Int, Value::I64(_))
        | (TypeNode::BigInt, Value::BigInt(_))
        | (TypeNode::Bool, Value::Bool(_)) => true,
        (TypeNode::Nat, Value::I64(value)) => *value >= 0,
        (TypeNode::Vector { element, extent }, Value::Vector(values)) => {
            matches!(
                &**element,
                TypeNode::Float64 | TypeNode::Int | TypeNode::Nat
            ) && match extent {
                Some(emath_ir::Extent::Fixed(size)) => values.len() == *size,
                None => true,
                Some(emath_ir::Extent::Symbolic(_)) => false,
            }
        }
        _ => false,
    }
}

