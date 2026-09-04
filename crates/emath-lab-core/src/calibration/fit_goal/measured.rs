//! Measured-data materialization and the rank oracle.

use super::*;

/// Typed refusal for [`materialize_measured`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FitMeasuredError {
    /// The confidence verdict did not cover a declared parameter.
    MissingDirection {
        /// The parameter without a direction.
        name: String,
    },
}

/// Materializes fitted parameter values into [`Measured`] evidence with
/// linked `Fitted` provenance (the content-addressed fit hash as
/// `fit_id`, 16 hex digits).
///
/// Uncertainty discipline: with a confidence verdict every declared
/// direction must appear (a missing direction is a typed refusal), and
/// std_uncertainty is `(hi - lo) / 3.92` — the normal-95 half-width
/// conversion. Without a verdict (`confidence: None`) the fit certifies
/// NO uncertainty: std_uncertainty is 0.0 as an explicit *unclaimed*
/// marker, never as a claim of zero error.
#[must_use]
pub fn materialize_measured(
    goal: &FitGoal,
    parameters: &BTreeMap<SymbolId, f64>,
    hash: ProvenanceHash,
    confidence: Option<&Identifiability>,
) -> Result<Vec<(SymbolId, Measured<f64>)>, FitMeasuredError> {
    let fit_id = format!("{:016x}", hash.0);
    let mut measured = Vec::with_capacity(goal.parameters.len());
    for parameter in &goal.parameters {
        let value = parameters.get(parameter).copied().unwrap_or(0.0);
        let std_uncertainty = match confidence {
            Some(verdict) => {
                let interval = verdict
                    .directions
                    .iter()
                    .find(|(symbol, _)| symbol == parameter)
                    .map(|(_, interval)| interval)
                    .ok_or_else(|| FitMeasuredError::MissingDirection {
                        name: parameter.0.clone(),
                    })?;
                ((interval.hi - interval.lo) / 3.92).max(0.0)
            }
            None => 0.0,
        };
        measured.push((
            parameter.clone(),
            Measured::new(
                value,
                std_uncertainty,
                DistributionKind::Normal,
                Provenance::Fitted {
                    fit_id: fit_id.clone(),
                },
                None,
                None,
            ),
        ));
    }
    Ok(measured)
}

/// Numeric local-rank structural-identifiability oracle (04 §5.3): an
/// honest executable rank oracle evaluated AT the fitted parameters.
/// The residual Jacobian's column rank (singular values of `J^T J`,
/// thresholded relative to the largest) decides how many directions the
/// data can distinguish:
///
/// - full rank: per-direction confidence intervals from the
///   covariance approximation `cov = sse/(m-n) * inv(J^T J)`,
///   normal-95 half-width `1.96 * sqrt(diag(cov))`; a direction is
///   TIGHT when its interval does not straddle zero (the data certifies
///   the sign, so escalation may grant);
/// - rank deficient: every direction is reported relaxed
///   (`-inf .. inf`), so escalation refuses naming the first one —
///   no authority is claimed;
/// - `rows <= parameters` (underdetermined normal system): the oracle
///   serves no verdict (`None`), the honest unresolved disposition.
#[derive(Debug, Clone, Copy)]
pub struct NumericRankOracle {
    /// Singular-value threshold relative to the largest; singular
    /// values at or below `rel_tolerance * sigma_max` count as zero
    /// rank.
    pub rel_tolerance: f64,
}

impl Default for NumericRankOracle {
    fn default() -> Self {
        Self {
            rel_tolerance: 1e-9,
        }
    }
}

impl IdentifiabilityProvider for NumericRankOracle {
    fn structural_identifiability(
        &self,
        goal: &FitGoal,
        model: &dyn FitModel,
        data: &[FitRow],
        fitted: &BTreeMap<SymbolId, f64>,
    ) -> Option<Identifiability> {
        let width = goal.parameters.len();
        let rows = data.len();
        if rows <= width {
            return None;
        }
        let jacobian = jacobian_residuals(goal, model, data, fitted).ok()?;
        let normal = normal_equations(goal, &jacobian, 0.0);
        let mut eigenvalues = jacobi_eigenvalues(&normal, width, 200);
        eigenvalues.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
        let sigma_max = eigenvalues[0].max(0.0).sqrt();
        let rank = eigenvalues
            .iter()
            .filter(|lambda| lambda.max(0.0).sqrt() > self.rel_tolerance * sigma_max)
            .count();
        if rank < width {
            return Some(Identifiability {
                directions: goal
                    .parameters
                    .iter()
                    .cloned()
                    .map(|symbol| {
                        (
                            symbol,
                            ConfidenceInterval {
                                lo: f64::NEG_INFINITY,
                                hi: f64::INFINITY,
                                tight: false,
                            },
                        )
                    })
                    .collect(),
            });
        }
        let residuals = weighted_residuals(goal, model, data, fitted).ok()?;
        let sse = residuals.iter().map(|r| r * r).sum::<f64>();
        let residual_variance = sse / (rows as f64 - width as f64);
        let inverse = invert_symmetric(&normal, width)?;
        let mut directions = Vec::with_capacity(width);
        for (index, parameter) in goal.parameters.iter().enumerate() {
            let variance =
                (residual_variance * inverse[index * width + index]).max(0.0);
            let sigma = variance.sqrt();
            let half = 1.96 * sigma;
            let value = fitted.get(parameter).copied().unwrap_or(0.0);
            directions.push((
                parameter.clone(),
                ConfidenceInterval {
                    lo: value - half,
                    hi: value + half,
                    tight: half.is_finite() && (value - half) * (value + half) > 0.0,
                },
            ));
        }
        Some(Identifiability { directions })
    }
}
