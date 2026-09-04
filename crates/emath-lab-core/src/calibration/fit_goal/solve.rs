//! Residual assembly, Levenberg-style fitting, escalation.

use super::*;

/// Weighted residual vector under the declared residual method
/// (`weighted_least_squares`): `row.weight * (predict - y)`. Per-
/// parameter weights scale the Jacobian, not the residual itself.
/// Model evaluation faults propagate as `Err`.
#[must_use]
pub fn weighted_residuals(
    goal: &FitGoal,
    model: &dyn FitModel,
    data: &[FitRow],
    parameters: &BTreeMap<SymbolId, f64>,
) -> Result<Vec<f64>, String> {
    data.iter()
        .map(|row| {
            model
                .predict(parameters, row.t)
                .map(|predicted| row.weight * (predicted - row.y))
        })
        .collect()
}

/// Numeric residual Jacobian (central differences over the model seam),
/// row-major over `data` rows with one column per declared parameter
/// (order fixed by `goal.parameters`, independent of map iteration).
/// Column `p` carries `weights.weight(p)` — the explicit per-parameter
/// weighting. Model evaluation faults propagate as `Err`.
#[must_use]
pub fn jacobian_residuals(
    goal: &FitGoal,
    model: &dyn FitModel,
    data: &[FitRow],
    parameters: &BTreeMap<SymbolId, f64>,
) -> Result<Vec<f64>, String> {
    let step = 1e-6;
    let width = goal.parameters.len();
    let mut jacobian = Vec::with_capacity(data.len() * width);
    for row in data {
        for parameter in &goal.parameters {
            let value = parameters.get(parameter).copied().unwrap_or(0.0);
            let mut shifted = parameters.clone();
            shifted.insert(parameter.clone(), value + step);
            let plus = model.predict(&shifted, row.t)?;
            shifted.insert(parameter.clone(), value - step);
            let minus = model.predict(&shifted, row.t)?;
            let derivative = (plus - minus) / (2.0 * step);
            jacobian.push(row.weight * goal.weights.weight(parameter) * derivative);
        }
    }
    Ok(jacobian)
}

/// Levenberg-Marquardt fit over the generic model seam (04 §5.3
/// `method levenberg_marquardt`). The fit never silently claims
/// structural identifiability: with `require_identifiability` set, a
/// missing provider yields [`FitOutcome::Unresolved`] and a provider
/// that reports a relaxed direction yields
/// [`FitOutcome::AuthorityRefused`] naming it.
#[must_use]
pub fn fit(
    goal: &FitGoal,
    model: &dyn FitModel,
    data: &[FitRow],
    identifiability: Option<&dyn IdentifiabilityProvider>,
) -> FitOutcome {
    if data.is_empty() {
        return FitOutcome::Unresolved {
            reason: UnresolvedReason::NoData,
        };
    }
    let mut parameters: BTreeMap<SymbolId, f64> = goal.initial.clone();
    let mut lambda = 1e-3;
    let mut previous_sse = f64::INFINITY;
    for _ in 0..256 {
        let residuals = match weighted_residuals(goal, model, data, &parameters) {
            Ok(residuals) => residuals,
            Err(detail) => return FitOutcome::ModelError { detail },
        };
        let sse = residuals.iter().map(|r| r * r).sum::<f64>();
        // Non-finite residuals/SSE mean the model left its domain: the
        // fit refuses (the documented never-NaN-poison contract), it
        // never optimizes over or returns poisoned values.
        if !sse.is_finite() {
            return FitOutcome::ModelError {
                detail: format!("non-finite residual sum of squares ({sse})"),
            };
        }
        if (previous_sse - sse).abs() <= 1e-12 * sse.abs().max(1.0) {
            break;
        }
        let jacobian = match jacobian_residuals(goal, model, data, &parameters) {
            Ok(jacobian) => jacobian,
            Err(detail) => return FitOutcome::ModelError { detail },
        };
        // A finite SSE with a non-finite Jacobian (finite at the point,
        // NaN at a finite-difference offset) would stall on rejected
        // steps and silently return the seed as a "fit"; refuse.
        if jacobian.iter().any(|value| !value.is_finite()) {
            return FitOutcome::ModelError {
                detail: "non-finite Jacobian entry (model undefined at a finite-difference \
                         offset)"
                    .to_string(),
            };
        }
        let normal = normal_equations(goal, &jacobian, lambda);
        let Some(step) = solve_normal(goal, &normal, &jacobian, &residuals) else {
            lambda *= 10.0;
            continue;
        };
        let candidate = apply_step(goal, &parameters, &step);
        let candidate_residuals = match weighted_residuals(goal, model, data, &candidate) {
            Ok(residuals) => residuals,
            Err(detail) => return FitOutcome::ModelError { detail },
        };
        let candidate_sse = candidate_residuals.iter().map(|r| r * r).sum::<f64>();
        if candidate_sse < sse {
            parameters = candidate;
            lambda = (lambda / 10.0).max(1e-12);
            previous_sse = sse;
        } else {
            lambda *= 10.0;
            if lambda > 1e12 {
                break;
            }
        }
    }
    let hash = provenance(goal, data, &parameters);
    if !goal.require_identifiability {
        return FitOutcome::Fitted {
            parameters,
            hash,
            confidence: None,
        };
    }
    let Some(provider) = identifiability else {
        return FitOutcome::Unresolved {
            reason: UnresolvedReason::SymbolicOracleUnavailable,
        };
    };
    let verdict = provider.structural_identifiability(goal, model, data, &parameters);
    match escalate(goal, hash, verdict.clone()) {
        AuthorityEscalation::Granted(hash) => FitOutcome::Fitted {
            parameters,
            hash,
            confidence: verdict,
        },
        AuthorityEscalation::Refused { direction, reason } => {
            FitOutcome::AuthorityRefused { direction, reason }
        }
    }
}

/// Escalation disposition from the identifiability verdict: granted when
/// every declared parameter has a direction AND every direction is
/// tight; refused naming the first relaxed or missing direction when one
/// exists, or when the check stayed unresolved (no provider).
#[must_use]
pub fn escalate(
    goal: &FitGoal,
    hash: ProvenanceHash,
    verdict: Option<Identifiability>,
) -> AuthorityEscalation {
    let Some(verdict) = verdict else {
        return AuthorityEscalation::Refused {
            direction: goal
                .parameters
                .first()
                .cloned()
                .unwrap_or_else(|| SymbolId("param".into())),
            reason: UnresolvedReason::SymbolicOracleUnavailable,
        };
    };
    for parameter in &goal.parameters {
        if !verdict.directions.iter().any(|(symbol, _)| symbol == parameter) {
            return AuthorityEscalation::Refused {
                direction: parameter.clone(),
                reason: UnresolvedReason::SymbolicOracleUnavailable,
            };
        }
    }
    for (symbol, interval) in &verdict.directions {
        if !interval.tight {
            return AuthorityEscalation::Refused {
                direction: symbol.clone(),
                reason: UnresolvedReason::StructureNotIdentifiable,
            };
        }
    }
    AuthorityEscalation::Granted(hash)
}

/// Normal equations `J^T W J + lambda I` (dense over the declared
/// parameter order).
pub(super) fn normal_equations(goal: &FitGoal, jacobian: &[f64], lambda: f64) -> Vec<f64> {
    let width = goal.parameters.len();
    let rows = jacobian.len() / width;
    let mut normal = vec![0.0; width * width];
    for row in 0..rows {
        for i in 0..width {
            let left = jacobian[row * width + i];
            for j in 0..width {
                normal[i * width + j] += left * jacobian[row * width + j];
            }
        }
    }
    for k in 0..width {
        normal[k * width + k] += lambda;
    }
    normal
}

/// Solves the damped normal system `normal * step = J^T residuals`
/// (Gaussian elimination with partial pivoting); `None` on a singular
/// system.
pub(super) fn solve_normal(
    goal: &FitGoal,
    normal: &[f64],
    jacobian: &[f64],
    residuals: &[f64],
) -> Option<Vec<f64>> {
    let width = goal.parameters.len();
    let rows = residuals.len();
    let mut gradient = vec![0.0; width];
    for row in 0..rows {
        for column in 0..width {
            gradient[column] += residuals[row] * jacobian[row * width + column];
        }
    }
    // Augmented matrix `[normal | gradient]`, row-interleaved so the
    // elimination stride logic below is `[N | g]` per row.
    let mut augmented = Vec::with_capacity(width * (width + 1));
    for row in 0..width {
        for column in 0..width {
            augmented.push(normal[row * width + column]);
        }
        augmented.push(gradient[row]);
    }
    for column in 0..width {
        let mut best = column;
        for row in column..width {
            let stride = width + 1;
            if augmented[row * stride + column].abs()
                > augmented[best * stride + column].abs()
            {
                best = row;
            }
        }
        let stride = width + 1;
        if augmented[best * stride + column].abs() < f64::EPSILON {
            return None;
        }
        augmented.swap(column, best);
        for row in column + 1..width {
            let factor =
                augmented[row * stride + column] / augmented[column * stride + column];
            for k in column..=width {
                let value = augmented[row * stride + k]
                    - factor * augmented[column * stride + k];
                augmented[row * stride + k] = value;
            }
        }
    }
    let mut solution = vec![0.0; width];
    let stride = width + 1;
    for column in (0..width).rev() {
        let mut value = augmented[column * stride + width];
        for k in column + 1..width {
            value -= augmented[column * stride + k] * solution[k];
        }
        solution[column] = value / augmented[column * stride + column];
    }
    Some(solution)
}

/// `parameters[p] -= step[p]` for each declared parameter.
pub(super) fn apply_step(
    goal: &FitGoal,
    parameters: &BTreeMap<SymbolId, f64>,
    step: &[f64],
) -> BTreeMap<SymbolId, f64> {
    let mut next = parameters.clone();
    for (index, parameter) in goal.parameters.iter().enumerate() {
        let value = parameters.get(parameter).copied().unwrap_or(0.0);
        next.insert(parameter.clone(), value - step[index]);
    }
    next
}

/// Content hash of the fitted parameters under the goal's vocabulary +
/// data + seed + method (FNV-1a 64; deterministic, dependency-free).
#[must_use]
pub fn provenance(
    goal: &FitGoal,
    data: &[FitRow],
    fitted: &BTreeMap<SymbolId, f64>,
) -> ProvenanceHash {
    let mut canonical = String::new();
    canonical.push_str("fit:");
    for parameter in &goal.parameters {
        canonical.push_str(&parameter.0);
        canonical.push(',');
    }
    canonical.push_str(&goal.observable.0);
    canonical.push(';');
    canonical.push_str(&goal.model.join("::"));
    canonical.push(';');
    canonical.push_str(&goal.prediction);
    canonical.push(';');
    canonical.push_str(goal.residual.as_str());
    canonical.push(';');
    canonical.push_str(goal.method.as_str());
    canonical.push(';');
    // Declared per-parameter weights steer the Jacobian: they are part
    // of the fit program, so two programs differing only in declared
    // weights are different programs and must not share a hash.
    for (name, value) in &goal.weights.0 {
        canonical.push_str(&name.0);
        canonical.push('=');
        canonical.push_str(&value.to_bits().to_string());
        canonical.push(',');
    }
    canonical.push(';');
    for (name, value) in &goal.initial {
        canonical.push_str(&name.0);
        canonical.push('=');
        canonical.push_str(&value.to_bits().to_string());
        canonical.push(',');
    }
    canonical.push('|');
    for row in data {
        canonical.push_str(&row.t.to_bits().to_string());
        canonical.push(',');
        canonical.push_str(&row.y.to_bits().to_string());
        canonical.push(',');
        canonical.push_str(&row.weight.to_bits().to_string());
        canonical.push(';');
    }
    canonical.push('|');
    for (name, value) in fitted {
        canonical.push_str(&name.0);
        canonical.push('=');
        canonical.push_str(&value.to_bits().to_string());
        canonical.push(',');
    }
    ProvenanceHash(fnv1a64(canonical.as_bytes()))
}
