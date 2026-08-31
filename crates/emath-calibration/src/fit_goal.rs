//! Generic fit-goal data and execution seams (bead emath-r3-fit-goal-4xjh,
//! 04 §5.3). Domain-free: this module knows parameters, observables,
//! residual methods, optimizer methods, weights, and content-addressed
//! provenance — never a concrete model. The PK two-compartment model is
//! a runnable `.emath` fixture (`language/examples/science/
//! pk-two-compartment-fit.emath`); execution here goes through
//! capability/method/provider seams:
//!
//! - residuals through [`ResidualMethod`] + [`FitModel`];
//! - optimization through [`OptimizerMethod`];
//! - structural identifiability through [`IdentifiabilityProvider`].
//!
//! Without a structural-identifiability provider the disposition is an
//! honest typed [`UnresolvedReason`]: no authority is claimed and
//! escalation refuses ([`AuthorityEscalation::Refused`]) instead of
//! silently claiming a fit. Fitting is estimation with uncertainty,
//! provenance, and identifiability — never bare optimization.

use std::collections::BTreeMap;

use emath_ir::goal::GoalPayload;
use emath_ir::provenance::{DistributionKind, Measured, Provenance};
use emath_term::SymbolId;

/// A generic fit program: vocabulary only. The `model` path and
/// `prediction` label name the surface used by a runnable `.emath`
/// fixture; no Rust code is bound to them here.
#[derive(Debug, Clone, PartialEq)]
pub struct FitGoal {
    /// Parameters to fit, in declared order (`fit k_el, V_central to
    /// conc_time` order).
    pub parameters: Vec<SymbolId>,
    /// Observable the residuals are measured against.
    pub observable: SymbolId,
    /// Model path as declared (`model PK_TwoCompartment`).
    pub model: Vec<String>,
    /// Prediction label as declared (`prediction [central]`).
    pub prediction: String,
    /// Declared residual method (capability seam).
    pub residual: ResidualMethod,
    /// Declared optimizer method (method seam).
    pub method: OptimizerMethod,
    /// Seed parameter values (`initial: k_el = 0.2`). Part of the
    /// provenance preimage — the fit is reproducible from the seed.
    pub initial: BTreeMap<SymbolId, f64>,
    /// Explicit per-parameter Jacobian weights (`weights: k_el = 2.0`
    /// scales the `k_el` Jacobian column; residuals are scaled by
    /// per-row weights only — see [`jacobian_residuals`]); never
    /// silent.
    pub weights: ResidualWeights,
    /// Observed data rows declared by the fit program (`data: t =
    /// [...], data: <observable> = [...]`), materialized at trace time
    /// with uniform row weight 1.0 (no per-row weights are declared in
    /// the fit program; per-parameter weighting is `weights`).
    pub data: Vec<FitRow>,
    /// Name of the data row that carries the independent coordinate
    /// (the model input `t` varies over). `""` on hand-built goals
    /// ([`FitGoal::new`]).
    pub coordinate: String,
    /// `require identifiability: structural` honesty gate: without a
    /// structural-identifiability provider the fit refuses to claim
    /// authority.
    pub require_identifiability: bool,
}

impl FitGoal {
    /// Builds the minimal goal: declared parameter order + observable.
    /// Every other field keeps its explicit default; a real fit program
    /// names model, prediction, residual, method, initial, and weights.
    #[must_use]
    pub fn new(parameters: Vec<SymbolId>, observable: SymbolId) -> Self {
        Self {
            parameters,
            observable,
            model: Vec::new(),
            prediction: String::new(),
            residual: ResidualMethod::WeightedLeastSquares,
            method: OptimizerMethod::LevenbergMarquardt,
            initial: BTreeMap::new(),
            weights: ResidualWeights::default(),
            data: Vec::new(),
            coordinate: String::new(),
            require_identifiability: false,
        }
    }

    /// Traces an elaborated fit payload (`emath_ir::goal::GoalPayload`,
    /// produced by the syntax/admission/lowering lane) plus its request
    /// target (the observable) into the generic runtime goal. The whole
    /// fit program is plain data — nothing domain-specific is bound
    /// here. Unknown method spellings and unparseable seed/weight
    /// literals are typed refusals, never silent defaults.
    pub fn from_payload(payload: &GoalPayload, observable: &str) -> Result<Self, FitPayloadError> {
        if payload.parameters.is_empty() {
            return Err(FitPayloadError::MissingParameters);
        }
        if observable.is_empty() {
            return Err(FitPayloadError::MissingObservable);
        }
        let residual = match payload.residual.as_str() {
            "weighted_least_squares" => ResidualMethod::WeightedLeastSquares,
            "" => return Err(FitPayloadError::MissingResidual),
            other => {
                return Err(FitPayloadError::UnknownResidual(other.to_string()));
            }
        };
        let method = match payload.method.as_str() {
            "levenberg_marquardt" => OptimizerMethod::LevenbergMarquardt,
            "" => return Err(FitPayloadError::MissingMethod),
            other => {
                return Err(FitPayloadError::UnknownMethod(other.to_string()));
            }
        };
        let mut initial = BTreeMap::new();
        for (name, literal) in &payload.initial {
            if !payload.parameters.iter().any(|parameter| parameter == name) {
                return Err(FitPayloadError::UnknownInitialParameter {
                    name: name.clone(),
                });
            }
            let value = literal
                .parse::<f64>()
                .map_err(|_| FitPayloadError::UnparseableNumber {
                    row: "initial".to_string(),
                    name: name.clone(),
                    literal: literal.clone(),
                })?;
            initial.insert(SymbolId(name.clone()), value);
        }
        let mut weights = BTreeMap::new();
        for (name, literal) in &payload.weights {
            if !payload.parameters.iter().any(|parameter| parameter == name) {
                return Err(FitPayloadError::UnknownWeightParameter {
                    name: name.clone(),
                });
            }
            let value = literal
                .parse::<f64>()
                .map_err(|_| FitPayloadError::UnparseableNumber {
                    row: "weights".to_string(),
                    name: name.clone(),
                    literal: literal.clone(),
                })?;
            if !(value > 0.0) {
                return Err(FitPayloadError::NonPositiveWeight {
                    name: name.clone(),
                    literal: literal.clone(),
                });
            }
            weights.insert(SymbolId(name.clone()), value);
        }
        let (coordinate, data) = fit_rows_from_payload(observable, &payload.data)?;
        Ok(Self {
            parameters: payload
                .parameters
                .iter()
                .map(|name| SymbolId(name.clone()))
                .collect(),
            observable: SymbolId(observable.to_string()),
            model: payload.model.clone(),
            prediction: payload.prediction.clone(),
            residual,
            method,
            initial,
            weights: ResidualWeights(weights),
            data,
            coordinate,
            require_identifiability: payload.require_identifiability,
        })
    }
}

/// Materializes the fit program's declared data rows into [`FitRow`]s:
/// one row must name the observable (`y`), exactly one other row names
/// the independent coordinate (`t`), and both must have equal arity.
/// Row weights are uniform 1.0 — the fit program declares per-parameter
/// weights only ([`FitGoal::weights`]). Returns the coordinate row name
/// alongside the rows.
fn fit_rows_from_payload(
    observable: &str,
    data: &[(String, Vec<String>)],
) -> Result<(String, Vec<FitRow>), FitPayloadError> {
    if data.is_empty() {
        return Err(FitPayloadError::MissingData);
    }
    let mut coordinates = Vec::new();
    let mut y_row = None;
    for (name, literals) in data {
        if name == observable {
            // A second row naming the observable refuses: silent
            // last-wins would let a typo'd duplicate silently replace
            // the measured data (the single-coordinate rule refuses
            // `TooManyCoordinateRows`; the observable row deserves the
            // same strictness).
            if y_row.is_some() {
                return Err(FitPayloadError::DuplicateObservableRow {
                    observable: observable.to_string(),
                });
            }
            y_row = Some((name, literals));
        } else {
            coordinates.push((name, literals));
        }
    }
    let y_row = y_row.ok_or_else(|| FitPayloadError::MissingObservableRow {
        observable: observable.to_string(),
    })?;
    let (coordinate_name, coordinate_literals) = match coordinates.as_slice() {
        [single] => single,
        [_, (extra, _), ..] => {
            return Err(FitPayloadError::TooManyCoordinateRows {
                observable: observable.to_string(),
                extra: extra.to_string(),
            });
        }
        [] => {
            return Err(FitPayloadError::MissingCoordinateRow {
                observable: observable.to_string(),
            });
        }
    };
    let coordinate = parse_data_row(coordinate_name, coordinate_literals)?;
    if coordinate.is_empty() {
        return Err(FitPayloadError::EmptyData);
    }
    let y = parse_data_row(y_row.0, y_row.1)?;
    if coordinate.len() != y.len() {
        return Err(FitPayloadError::DataLengthMismatch {
            coordinate: coordinate_name.to_string(),
            coordinate_len: coordinate.len(),
            observable: y_row.0.to_string(),
            observable_len: y.len(),
        });
    }
    Ok((
        coordinate_name.to_string(),
        coordinate
            .into_iter()
            .zip(y)
            .map(|(t, y)| FitRow { t, y, weight: 1.0 })
            .collect(),
    ))
}

fn parse_data_row(name: &str, literals: &[String]) -> Result<Vec<f64>, FitPayloadError> {
    literals
        .iter()
        .map(|literal| {
            literal.parse::<f64>().map_err(|_| {
                FitPayloadError::UnparseableNumber {
                    row: format!("data:{name}"),
                    name: name.to_string(),
                    literal: literal.clone(),
                }
            })
        })
        .collect()
}

/// Typed refusal for a malformed or unknown fit payload
/// ([`FitGoal::from_payload`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FitPayloadError {
    /// The payload carried no parameters.
    MissingParameters,
    /// No observable was supplied for the goal.
    MissingObservable,
    /// The payload declared no data rows.
    MissingData,
    /// No data row names the observable.
    MissingObservableRow {
        /// The observable the fit targets.
        observable: String,
    },
    /// More than one data row names the observable; a duplicate would
    /// silently replace the measured data.
    DuplicateObservableRow {
        /// The observable the fit targets.
        observable: String,
    },
    /// No data row names an independent coordinate (the `t` row).
    MissingCoordinateRow {
        /// The observable the fit targets.
        observable: String,
    },
    /// More than one data row names a coordinate.
    TooManyCoordinateRows {
        /// The observable the fit targets.
        observable: String,
        /// The first extra coordinate row name.
        extra: String,
    },
    /// The coordinate and observable rows disagree in arity.
    DataLengthMismatch {
        /// Coordinate row name.
        coordinate: String,
        /// Coordinate row arity.
        coordinate_len: usize,
        /// Observable row name.
        observable: String,
        /// Observable row arity.
        observable_len: usize,
    },
    /// A coordinate row with zero entries.
    EmptyData,
    /// A seed names a parameter that is not in the fit program.
    UnknownInitialParameter {
        /// The unknown parameter name.
        name: String,
    },
    /// A weight names a parameter that is not in the fit program.
    UnknownWeightParameter {
        /// The unknown parameter name.
        name: String,
    },
    /// A weight literal is not strictly positive.
    NonPositiveWeight {
        /// The parameter name.
        name: String,
        /// The offending literal.
        literal: String,
    },
    /// No residual method was declared.
    MissingResidual,
    /// The residual method spelling is unknown.
    UnknownResidual(String),
    /// No optimizer method was declared.
    MissingMethod,
    /// The optimizer method spelling is unknown.
    UnknownMethod(String),
    /// A seed, weight, or data literal did not parse as a number.
    UnparseableNumber {
        /// `initial`, `weights`, or `data:<row>`.
        row: String,
        /// Parameter or data entry name.
        name: String,
        /// The literal that failed to parse.
        literal: String,
    },
}

/// Explicit Jacobian weights keyed by parameter symbol; never silent.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ResidualWeights(pub BTreeMap<SymbolId, f64>);

impl ResidualWeights {
    /// Weight for one parameter; absent rows weight 1.0 (declared
    /// explicitly, not implied).
    #[must_use]
    pub fn weight(&self, symbol: &SymbolId) -> f64 {
        self.0.get(symbol).copied().unwrap_or(1.0)
    }
}

/// Declared residual method (capability seam).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResidualMethod {
    /// `residual: weighted_least_squares` — explicit per-row and
    /// per-parameter weights; nothing is weighted silently.
    WeightedLeastSquares,
}

impl ResidualMethod {
    /// Stable surface spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::WeightedLeastSquares => "weighted_least_squares",
        }
    }
}

/// Declared optimizer method (method seam).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptimizerMethod {
    /// `method levenberg_marquardt` — damped least squares over the
    /// numeric Jacobian of the model seam.
    LevenbergMarquardt,
}

impl OptimizerMethod {
    /// Stable surface spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LevenbergMarquardt => "levenberg_marquardt",
        }
    }
}

/// One weighted observation row of the observable.
#[derive(Debug, Clone, PartialEq)]
pub struct FitRow {
    /// Point of the observable (h for the PK fixture).
    pub t: f64,
    /// Observed value.
    pub y: f64,
    /// Explicit residual weight; never silent.
    pub weight: f64,
}

/// Model provider seam: predict the observable at `t` under
/// `parameters`. The concrete model (e.g. the PK fixture) lives in
/// `.emath`, not here. A `Result` keeps evaluation faults (domain
/// errors, interpreter refusals) visible: the fit refuses, it never
/// NaN-poisons the optimizer.
pub trait FitModel {
    /// Predicted observable value.
    fn predict(&self, parameters: &BTreeMap<SymbolId, f64>, t: f64) -> Result<f64, String>;
}

/// Structural-identifiability provider seam. `None` means the provider
/// does not serve this goal — the honest unresolved disposition. The
/// provider receives the model, data, and fitted parameters so a
/// numeric oracle can evaluate identifiability at the solution.
pub trait IdentifiabilityProvider {
    /// Structural identifiability verdict for the goal, when a provider
    /// exists.
    fn structural_identifiability(
        &self,
        goal: &FitGoal,
        model: &dyn FitModel,
        data: &[FitRow],
        fitted: &BTreeMap<SymbolId, f64>,
    ) -> Option<Identifiability>;
}

/// Structural identifiability result with named directions.
#[derive(Debug, Clone, PartialEq)]
pub struct Identifiability {
    /// Named directions with their confidence intervals.
    pub directions: Vec<(SymbolId, ConfidenceInterval)>,
}

/// Confidence interval of one fitted direction.
#[derive(Debug, Clone, PartialEq)]
pub struct ConfidenceInterval {
    /// Lower bound.
    pub lo: f64,
    /// Upper bound.
    pub hi: f64,
    /// Whether the interval is tight (identifiable) or relaxed.
    pub tight: bool,
}

/// Fit outcome: fitted parameters with provenance, an honest typed
/// unresolved disposition when no structural-identifiability provider
/// exists, or a refusal naming the direction that stayed
/// unidentifiable.
#[derive(Debug, Clone, PartialEq)]
pub enum FitOutcome {
    /// Converged numeric fit with content-addressed provenance (model +
    /// vocabulary + data + seed + method). Every parameter name is
    /// bound; no authority is claimed over identifiability.
    Fitted {
        /// Fitted parameter bindings, one per declared parameter.
        parameters: BTreeMap<SymbolId, f64>,
        /// Content-addressed provenance hash.
        hash: ProvenanceHash,
        /// Per-direction confidence verdict when the fit ran under a
        /// structural-identifiability provider and escalation granted;
        /// `None` when no identifiability was claimed (the fit then
        /// certifies no uncertainty — `materialize_measured` reports
        /// std_uncertainty 0.0 as *unclaimed*, never as zero error).
        confidence: Option<Identifiability>,
    },
    /// Escalation refused for one direction; the confidence interval is
    /// reported unclaimed, never silently optimized away.
    AuthorityRefused {
        /// Direction that refused escalation.
        direction: SymbolId,
        /// Why the escalation was refused.
        reason: UnresolvedReason,
    },
    /// The model seam failed to predict (domain error, interpreter
    /// refusal); the fit refuses instead of optimizing over poisoned
    /// values.
    ModelError {
        /// What the model evaluation reported.
        detail: String,
    },
    /// The structural identifiability check could not run because no
    /// provider serves this goal; no authority is claimed.
    Unresolved {
        /// Why the check stayed unresolved.
        reason: UnresolvedReason,
    },
}

/// Typed unresolved disposition for a fit goal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnresolvedReason {
    /// The symbolic oracle provider was unavailable for this goal.
    SymbolicOracleUnavailable,
    /// A provider ran but a direction structurally fails
    /// identifiability.
    StructureNotIdentifiable,
    /// The fit was invoked with no data rows (direct API misuse).
    NoData,
}

/// AuthorityEscalation disposition of a fit: the fitted provenance hash
/// when structural identifiability is resolved and honest, or a typed
/// refusal naming the direction that stayed unidentifiable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthorityEscalation {
    /// Fitted provenance (model + data + seed + method, hashed).
    Granted(ProvenanceHash),
    /// Refused for one direction; the confidence interval is reported
    /// unclaimed, never silently optimized away.
    Refused {
        /// The direction that refused escalation.
        direction: SymbolId,
        /// Why the escalation was refused.
        reason: UnresolvedReason,
    },
}

/// Content-addressed `Fitted` provenance hash over model + vocabulary +
/// data + seed + method (FNV-1a 64; deterministic, dependency-free).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvenanceHash(pub u64);

/// FNV-1a 64-bit content hash.
#[must_use]
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf_29ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

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
fn normal_equations(goal: &FitGoal, jacobian: &[f64], lambda: f64) -> Vec<f64> {
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
fn solve_normal(
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
fn apply_step(
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

/// Eigenvalues of a real symmetric matrix via the classic Jacobi
/// rotation sweep (small dense matrices; deterministic, no
/// dependencies).
fn jacobi_eigenvalues(matrix: &[f64], n: usize, max_sweeps: usize) -> Vec<f64> {
    let mut a = matrix.to_vec();
    for _ in 0..max_sweeps {
        let mut p = 0;
        let mut q = 1;
        let mut largest = 0.0;
        for i in 0..n {
            for j in (i + 1)..n {
                let value = a[i * n + j].abs();
                if value > largest {
                    largest = value;
                    p = i;
                    q = j;
                }
            }
        }
        if largest < 1e-300 {
            break;
        }
        let app = a[p * n + p];
        let aqq = a[q * n + q];
        let apq = a[p * n + q];
        let theta = 0.5 * (aqq - app) / apq;
        let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
        let c = 1.0 / (t * t + 1.0).sqrt();
        let s = t * c;
        // Two-sided similarity transform A' = G^T A G: rotate the
        // off-block rows/columns from SAVED values (a later k iteration
        // must never read an entry an earlier one already overwrote),
        // then apply the closed-form 2x2 block update. The rotation
        // annihilates a[p][q]; reading the diagonal off `a` is then the
        // eigenvalue set.
        for k in 0..n {
            if k == p || k == q {
                continue;
            }
            let akp = a[k * n + p];
            let akq = a[k * n + q];
            a[k * n + p] = c * akp - s * akq;
            a[p * n + k] = a[k * n + p];
            a[k * n + q] = s * akp + c * akq;
            a[q * n + k] = a[k * n + q];
        }
        a[p * n + p] = c * c * app - 2.0 * s * c * apq + s * s * aqq;
        a[q * n + q] = s * s * app + 2.0 * s * c * apq + c * c * aqq;
        a[p * n + q] = 0.0;
        a[q * n + p] = 0.0;
    }
    (0..n).map(|i| a[i * n + i]).collect()
}

/// Inverse of a real symmetric matrix by Gauss-Jordan elimination with
/// partial pivoting; `None` when singular.
fn invert_symmetric(matrix: &[f64], n: usize) -> Option<Vec<f64>> {
    let stride = 2 * n;
    let mut augmented = vec![0.0; n * stride];
    for i in 0..n {
        for j in 0..n {
            augmented[i * stride + j] = matrix[i * n + j];
        }
        augmented[i * stride + n + i] = 1.0;
    }
    for column in 0..n {
        let mut best = column;
        for row in column..n {
            if augmented[row * stride + column].abs()
                > augmented[best * stride + column].abs()
            {
                best = row;
            }
        }
        if augmented[best * stride + column].abs() < f64::EPSILON {
            return None;
        }
        augmented.swap(column, best);
        let pivot = augmented[column * stride + column];
        for k in 0..stride {
            augmented[column * stride + k] /= pivot;
        }
        for row in 0..n {
            if row == column {
                continue;
            }
            let factor = augmented[row * stride + column];
            if factor == 0.0 {
                continue;
            }
            for k in 0..stride {
                augmented[row * stride + k] -= factor * augmented[column * stride + k];
            }
        }
    }
    let mut inverse = vec![0.0; n * n];
    for i in 0..n {
        for j in 0..n {
            inverse[i * n + j] = augmented[i * stride + n + j];
        }
    }
    Some(inverse)
}
