//! Fit-goal data model: goals, rows, methods, outcomes.

use super::*;

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
pub(super) fn fit_rows_from_payload(
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

pub(super) fn parse_data_row(name: &str, literals: &[String]) -> Result<Vec<f64>, FitPayloadError> {
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
