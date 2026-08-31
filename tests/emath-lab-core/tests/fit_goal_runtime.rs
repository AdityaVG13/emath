//! Failure-first generic fit-goal runtime tests (bead emath-r3-fit-goal-4xjh,
//! 04 §5.3). The Rust side is GENERIC fit data/plumbing only: parameters,
//! observable, residual method, optimizer method, weights, provenance —
//! no PK math. The PK model is a runnable `.emath` fixture
//! (`language/examples/science/pk-two-compartment-fit.emath`), proven by
//! the syntax suite. These tests exercise the capability/method/provider
//! seams and the honest typed unresolved disposition when no
//! structural-identifiability provider exists.

use std::collections::BTreeMap;

use emath_lab_core::calibration::{
    AuthorityEscalation, ConfidenceInterval, FitGoal, FitModel, FitOutcome, FitPayloadError,
    FitRow, Identifiability, IdentifiabilityProvider, OptimizerMethod, ProvenanceHash,
    ResidualMethod, ResidualWeights, UnresolvedReason, escalate, fit, jacobian_residuals,
    weighted_residuals,
};
use emath_ir::goal::GoalPayload;
use emath_term::SymbolId;

/// A generic two-parameter model for the seams (not the PK fixture —
/// the fixture lives in `.emath`): `slope * t + intercept`.
struct LinearModel;

impl FitModel for LinearModel {
    fn predict(&self, parameters: &BTreeMap<SymbolId, f64>, t: f64) -> Result<f64, String> {
        let slope = parameters
            .get(&SymbolId("slope".into()))
            .copied()
            .unwrap_or(0.0);
        let intercept = parameters
            .get(&SymbolId("intercept".into()))
            .copied()
            .unwrap_or(0.0);
        Ok(slope * t + intercept)
    }
}

fn slope() -> SymbolId {
    SymbolId("slope".into())
}

fn intercept() -> SymbolId {
    SymbolId("intercept".into())
}

/// `fit slope, intercept to response:` with explicit residual method,
/// optimizer method, seed, and weights.
fn fixture_goal(require_identifiability: bool) -> FitGoal {
    let mut goal = FitGoal::new(vec![slope(), intercept()], SymbolId("response".into()));
    goal.model = vec!["LinearModel".into()];
    goal.prediction = "value".into();
    goal.weights = ResidualWeights(BTreeMap::from([
        (slope(), 1.0),
        (intercept(), 1.0),
    ]));
    goal.initial = BTreeMap::from([(slope(), 1.0), (intercept(), 0.0)]);
    goal.require_identifiability = require_identifiability;
    goal
}

/// Exact data on `y = 2t + 1`.
fn fixture_data() -> Vec<FitRow> {
    vec![
        FitRow { t: 0.0, y: 1.0, weight: 1.0 },
        FitRow { t: 1.0, y: 3.0, weight: 1.0 },
        FitRow { t: 2.0, y: 5.0, weight: 1.0 },
        FitRow { t: 3.0, y: 7.0, weight: 1.0 },
    ]
}

#[test]
fn fit_executes_declared_program_generically_with_provenance() {
    let goal = fixture_goal(false);
    let data = fixture_data();
    let first = fit(&goal, &LinearModel, &data, None);
    let FitOutcome::Fitted { parameters, hash, .. } = &first else {
        panic!("the generic LM fit must materialize Fitted with provenance; got: {first:?}");
    };
    assert_ne!(hash.0, 0, "provenance hash must be nonzero");
    let fitted_slope = parameters
        .get(&slope())
        .copied()
        .expect("slope must be bound");
    let fitted_intercept = parameters
        .get(&intercept())
        .copied()
        .expect("intercept must be bound");
    assert!(
        (fitted_slope - 2.0).abs() < 1e-4,
        "slope must converge to 2.0; got {fitted_slope}"
    );
    assert!(
        (fitted_intercept - 1.0).abs() < 1e-4,
        "intercept must converge to 1.0; got {fitted_intercept}"
    );
    // Determinism: the same program + data + seed hashes identically.
    let second = fit(&goal, &LinearModel, &data, None);
    let FitOutcome::Fitted { hash: second_hash, .. } = second else {
        panic!("second fit must also be Fitted");
    };
    assert_eq!(
        hash, &second_hash,
        "identical fit programs must hash identically (determinism class)"
    );
}

#[test]
fn fit_unresolved_without_structural_identifiability_provider() {
    let goal = fixture_goal(true);
    let data = fixture_data();
    let outcome = fit(&goal, &LinearModel, &data, None);
    assert!(
        matches!(
            outcome,
            FitOutcome::Unresolved {
                reason: UnresolvedReason::SymbolicOracleUnavailable
            }
        ),
        "a fit that requires structural identifiability without a provider \
         must stay honestly unresolved; got: {outcome:?}"
    );
}

#[test]
fn identifiability_provider_seam_refuses_relaxed_direction() {
    let goal = fixture_goal(true);
    let data = fixture_data();
    // Provider reports: slope tight, intercept RELAXED → escalation must
    // refuse naming intercept, never silently claim the fit.
    struct LooseIntercept;
    impl IdentifiabilityProvider for LooseIntercept {
        fn structural_identifiability(
            &self,
            goal: &FitGoal,
            _model: &dyn FitModel,
            _data: &[FitRow],
            _fitted: &BTreeMap<SymbolId, f64>,
        ) -> Option<Identifiability> {
            Some(Identifiability {
                directions: vec![
                    (
                        goal.parameters[0].clone(),
                        ConfidenceInterval { lo: -0.01, hi: 0.01, tight: true },
                    ),
                    (
                        goal.parameters[1].clone(),
                        ConfidenceInterval { lo: -5.0, hi: 5.0, tight: false },
                    ),
                ],
            })
        }
    }
    let outcome = fit(&goal, &LinearModel, &data, Some(&LooseIntercept));
    assert!(
        matches!(
            outcome,
            FitOutcome::AuthorityRefused {
                ref direction,
                reason: UnresolvedReason::StructureNotIdentifiable,
            } if *direction == intercept()
        ),
        "a relaxed direction must refuse AuthorityEscalation naming it; got: {outcome:?}"
    );
}

#[test]
fn identifiability_provider_seam_grants_when_all_directions_tight() {
    let goal = fixture_goal(true);
    let data = fixture_data();
    struct AllTight;
    impl IdentifiabilityProvider for AllTight {
        fn structural_identifiability(
            &self,
            goal: &FitGoal,
            _model: &dyn FitModel,
            _data: &[FitRow],
            _fitted: &BTreeMap<SymbolId, f64>,
        ) -> Option<Identifiability> {
            Some(Identifiability {
                directions: goal
                    .parameters
                    .iter()
                    .cloned()
                    .map(|symbol| {
                        (
                            symbol,
                            ConfidenceInterval { lo: -0.01, hi: 0.01, tight: true },
                        )
                    })
                    .collect(),
            })
        }
    }
    let outcome = fit(&goal, &LinearModel, &data, Some(&AllTight));
    assert!(
        matches!(outcome, FitOutcome::Fitted { .. }),
        "all-tight directions grant escalation; got: {outcome:?}"
    );
    // The escalation helper's refusal spelling is the mirror image:
    // without any verdict there is no grant.
    assert!(
        matches!(
            escalate(&goal, ProvenanceHash(7), None),
            AuthorityEscalation::Refused {
                ref direction,
                reason: UnresolvedReason::SymbolicOracleUnavailable,
            } if *direction == slope()
        ),
        "a missing verdict must refuse escalation naming the first parameter"
    );
}

#[test]
fn residual_weights_are_explicit_and_scale_jacobian_columns() {
    let goal = fixture_goal(false);
    let data = fixture_data();
    let parameters = goal.initial.clone();
    // Residual vector: row.weight * (predict - y); row weights from data.
    let residuals =
        weighted_residuals(&goal, &LinearModel, &data, &parameters).expect("linear model");
    let expected: Vec<f64> = data
        .iter()
        .map(|row| row.weight * (1.0 * row.t + 0.0 - row.y))
        .collect();
    assert_eq!(residuals, expected, "weighted residual vector");
    // Per-parameter weights are explicit: doubling the slope weight
    // doubles its Jacobian column (data row 0, column 0).
    let mut weighted = goal.clone();
    weighted.weights = ResidualWeights(BTreeMap::from([
        (slope(), 2.0),
        (intercept(), 1.0),
    ]));
    let jacobian_plain =
        jacobian_residuals(&goal, &LinearModel, &data, &parameters).expect("linear model");
    let jacobian_weighted =
        jacobian_residuals(&weighted, &LinearModel, &data, &parameters).expect("linear model");
    for row in 0..data.len() {
        let col = row * goal.parameters.len();
        assert!(
            (jacobian_weighted[col] - 2.0 * jacobian_plain[col]).abs() < 1e-9,
            "slope column must scale by its declared weight"
        );
        assert_eq!(
            jacobian_weighted[col + 1],
            jacobian_plain[col + 1],
            "intercept column must keep its declared weight"
        );
    }
    assert_eq!(goal.residual, ResidualMethod::WeightedLeastSquares);
    assert_eq!(goal.method, OptimizerMethod::LevenbergMarquardt);
    assert_eq!(goal.residual.as_str(), "weighted_least_squares");
    assert_eq!(goal.method.as_str(), "levenberg_marquardt");
}

/// An elaborated fit payload exactly as the syntax/lowering lane
/// produces it for the runnable PK fixture (parameters in declared
/// order, model path, prediction label, residual/method spellings,
/// seed and weight literals, identifiability gate).
fn pk_fixture_payload() -> GoalPayload {
    let mut payload: GoalPayload = Default::default();
    payload.parameters = vec!["k_el".into(), "V_central".into()];
    payload.model = vec!["PK_TwoCompartment".into()];
    payload.prediction = "central".into();
    payload.residual = "weighted_least_squares".into();
    payload.method = "levenberg_marquardt".into();
    payload.initial = vec![
        ("k_el".into(), "0.2".into()),
        ("V_central".into(), "1.0".into()),
    ];
    payload.weights = vec![
        ("k_el".into(), "1.0".into()),
        ("V_central".into(), "1.0".into()),
    ];
    payload.data = vec![
        ("t".into(), vec!["0.5".into(), "1.0".into(), "2.0".into(), "4.0".into()]),
        ("conc_time".into(), vec!["2.41".into(), "1.93".into(), "1.24".into(), "0.64".into()]),
    ];
    payload.require_identifiability = true;
    payload
}

#[test]
fn fit_payload_traces_to_runtime_goal_losslessly() {
    let payload = pk_fixture_payload();
    let goal = FitGoal::from_payload(&payload, "conc_time").expect("payload must trace");
    assert_eq!(
        goal.parameters,
        vec![SymbolId("k_el".into()), SymbolId("V_central".into())],
        "parameter order from the fit head must survive"
    );
    assert_eq!(goal.observable, SymbolId("conc_time".into()));
    assert_eq!(goal.model, vec!["PK_TwoCompartment"]);
    assert_eq!(goal.prediction, "central");
    assert_eq!(goal.residual, ResidualMethod::WeightedLeastSquares);
    assert_eq!(goal.method, OptimizerMethod::LevenbergMarquardt);
    assert_eq!(
        goal.initial,
        BTreeMap::from([(SymbolId("k_el".into()), 0.2), (SymbolId("V_central".into()), 1.0)])
    );
    assert_eq!(
        goal.weights.0,
        BTreeMap::from([(SymbolId("k_el".into()), 1.0), (SymbolId("V_central".into()), 1.0)])
    );
    assert_eq!(
        goal.data,
        vec![
            FitRow { t: 0.5, y: 2.41, weight: 1.0 },
            FitRow { t: 1.0, y: 1.93, weight: 1.0 },
            FitRow { t: 2.0, y: 1.24, weight: 1.0 },
            FitRow { t: 4.0, y: 0.64, weight: 1.0 },
        ],
        "declared data rows materialize with uniform row weight 1.0"
    );
    assert_eq!(
        goal.coordinate, "t",
        "the data coordinate row name must survive the trace"
    );
    assert!(goal.require_identifiability, "honesty gate must survive");
}

#[test]
fn payload_trace_is_the_executable_program() {
    // The traced goal runs end-to-end on the generic model seam: the
    // program (parameters, seeds, weights, method, data) drives the fit.
    let payload = pk_fixture_payload();
    // Reuse the linear seam under the traced vocabulary and its declared
    // data rows.
    let mut goal = FitGoal::from_payload(&payload, "conc_time").expect("payload must trace");
    goal.parameters = vec![slope(), intercept()];
    goal.initial = BTreeMap::from([(slope(), 1.0), (intercept(), 0.0)]);
    goal.weights = ResidualWeights(BTreeMap::from([
        (slope(), 1.0),
        (intercept(), 1.0),
    ]));
    goal.data = fixture_data();
    goal.require_identifiability = false;
    let outcome = fit(&goal, &LinearModel, &goal.data, None);
    let FitOutcome::Fitted { parameters, .. } = &outcome else {
        panic!("traced goal must fit; got: {outcome:?}");
    };
    assert!(
        (parameters.get(&slope()).copied().unwrap_or(0.0) - 2.0).abs() < 1e-4,
        "traced slope must converge to 2.0"
    );
    assert!(
        (parameters.get(&intercept()).copied().unwrap_or(0.0) - 1.0).abs() < 1e-4,
        "traced intercept must converge to 1.0"
    );
}

#[test]
fn payload_trace_refuses_unknown_and_malformed_spellings() {
    let mut payload = pk_fixture_payload();
    payload.method = "firefly".into();
    assert_eq!(
        FitGoal::from_payload(&payload, "conc_time"),
        Err(FitPayloadError::UnknownMethod("firefly".into())),
        "unknown optimizer methods must refuse, never silently default"
    );
    let mut payload = pk_fixture_payload();
    payload.initial = vec![("k_el".into(), "abc".into())];
    assert_eq!(
        FitGoal::from_payload(&payload, "conc_time"),
        Err(FitPayloadError::UnparseableNumber {
            row: "initial".into(),
            name: "k_el".into(),
            literal: "abc".into(),
        }),
        "unparseable seed literals must refuse by name"
    );
    let mut payload = pk_fixture_payload();
    payload.parameters.clear();
    assert_eq!(
        FitGoal::from_payload(&payload, "conc_time"),
        Err(FitPayloadError::MissingParameters),
        "a fit program with no parameters must refuse"
    );
    let mut payload = pk_fixture_payload();
    payload.residual.clear();
    assert_eq!(
        FitGoal::from_payload(&payload, "conc_time"),
        Err(FitPayloadError::MissingResidual),
        "a fit program without an explicit residual must refuse"
    );
    let mut payload = pk_fixture_payload();
    payload.data.clear();
    assert_eq!(
        FitGoal::from_payload(&payload, "conc_time"),
        Err(FitPayloadError::MissingData),
        "a fit program without declared data must refuse"
    );
    let mut payload = pk_fixture_payload();
    payload.data = vec![("t".into(), vec!["0.5".into(), "1.0".into()])];
    assert_eq!(
        FitGoal::from_payload(&payload, "conc_time"),
        Err(FitPayloadError::MissingObservableRow {
            observable: "conc_time".into()
        }),
        "a data set without a row naming the observable must refuse"
    );
    let mut payload = pk_fixture_payload();
    payload.data = vec![
        ("t".into(), vec!["0.5".into(), "1.0".into()]),
        ("conc_time".into(), vec!["2.41".into(), "1.93".into()]),
        ("extra".into(), vec!["1.0".into()]),
    ];
    assert_eq!(
        FitGoal::from_payload(&payload, "conc_time"),
        Err(FitPayloadError::TooManyCoordinateRows {
            observable: "conc_time".into(),
            extra: "extra".into()
        }),
        "more than one coordinate row must refuse by name"
    );
    let mut payload = pk_fixture_payload();
    payload.data = vec![
        ("t".into(), vec!["0.5".into(), "1.0".into(), "2.0".into()]),
        ("conc_time".into(), vec!["2.41".into(), "1.93".into()]),
    ];
    assert_eq!(
        FitGoal::from_payload(&payload, "conc_time"),
        Err(FitPayloadError::DataLengthMismatch {
            coordinate: "t".into(),
            coordinate_len: 3,
            observable: "conc_time".into(),
            observable_len: 2
        }),
        "coordinate/observable arity mismatch must refuse with both lengths"
    );
    let mut payload = pk_fixture_payload();
    payload.data = vec![
        ("t".into(), vec!["0.5".into(), "abc".into()]),
        ("conc_time".into(), vec!["2.41".into(), "1.93".into()]),
    ];
    assert_eq!(
        FitGoal::from_payload(&payload, "conc_time"),
        Err(FitPayloadError::UnparseableNumber {
            row: "data:t".into(),
            name: "t".into(),
            literal: "abc".into()
        }),
        "an unparseable data literal must refuse naming the row and literal"
    );
}

/// A generic model where the two parameters enter as a SUM: structurally
/// collinear at every point — rank-deficient Jacobian.
struct SumModel;

impl FitModel for SumModel {
    fn predict(&self, parameters: &BTreeMap<SymbolId, f64>, _t: f64) -> Result<f64, String> {
        Ok(parameters.get(&slope()).copied().unwrap_or(0.0)
            + parameters.get(&intercept()).copied().unwrap_or(0.0))
    }
}

/// Slightly noisy data on `y = 2t + 1` (SSE > 0 so the covariance
/// approximation is meaningful).
fn noisy_fixture_data() -> Vec<FitRow> {
    vec![
        FitRow { t: 0.0, y: 1.01, weight: 1.0 },
        FitRow { t: 1.0, y: 3.02, weight: 1.0 },
        FitRow { t: 2.0, y: 4.99, weight: 1.0 },
        FitRow { t: 3.0, y: 7.0, weight: 1.0 },
    ]
}

#[test]
fn numeric_rank_oracle_serves_tight_full_rank_directions() {
    let goal = fixture_goal(true);
    let data = fixture_data();
    let oracle = emath_lab_core::calibration::NumericRankOracle::default();
    let outcome = fit(&goal, &LinearModel, &data, Some(&oracle));
    let FitOutcome::Fitted {
        parameters,
        confidence,
        ..
    } = &outcome
    else {
        panic!("full-rank exact data must grant escalation; got: {outcome:?}");
    };
    assert!(
        (parameters.get(&slope()).copied().unwrap_or(0.0) - 2.0).abs() < 1e-4
            && (parameters.get(&intercept()).copied().unwrap_or(0.0) - 1.0).abs() < 1e-4,
        "the oracle must not disturb the fit"
    );
    let verdict = confidence.as_ref().expect("granted fit carries its verdict");
    assert_eq!(verdict.directions.len(), 2, "one interval per declared direction");
    for (symbol, interval) in &verdict.directions {
        assert!(
            interval.tight && interval.lo.is_finite() && interval.hi.is_finite(),
            "full-rank directions must be tight and finite; {symbol:?}: {interval:?}"
        );
    }
}

#[test]
fn numeric_rank_oracle_refuses_collinear_directions() {
    let goal = fixture_goal(true);
    let data = vec![
        FitRow { t: 0.0, y: 3.0, weight: 1.0 },
        FitRow { t: 1.0, y: 3.1, weight: 1.0 },
        FitRow { t: 2.0, y: 2.9, weight: 1.0 },
    ];
    let oracle = emath_lab_core::calibration::NumericRankOracle::default();
    let outcome = fit(&goal, &SumModel, &data, Some(&oracle));
    assert!(
        matches!(
            outcome,
            FitOutcome::AuthorityRefused {
                ref direction,
                reason: UnresolvedReason::StructureNotIdentifiable,
            } if *direction == slope()
        ),
        "a rank-deficient Jacobian must refuse escalation naming the first \
         relaxed direction; got: {outcome:?}"
    );
}

#[test]
fn numeric_rank_oracle_cannot_certify_underdetermined_data() {
    let goal = fixture_goal(true);
    let data = vec![FitRow { t: 0.0, y: 1.0, weight: 1.0 }];
    let oracle = emath_lab_core::calibration::NumericRankOracle::default();
    let outcome = fit(&goal, &LinearModel, &data, Some(&oracle));
    assert!(
        matches!(
            outcome,
            FitOutcome::AuthorityRefused {
                ref direction,
                reason: UnresolvedReason::SymbolicOracleUnavailable,
            } if *direction == slope()
        ),
        "data that cannot certify all directions must refuse escalation with \
         the honest unresolved reason; got: {outcome:?}"
    );
}

#[test]
fn materialize_measured_links_fitted_values_with_content_hash() {
    let goal = fixture_goal(false);
    let data = noisy_fixture_data();
    let FitOutcome::Fitted {
        parameters,
        hash,
        confidence,
    } = fit(&goal, &LinearModel, &data, None)
    else {
        panic!("the fit must converge on noisy data");
    };
    assert!(
        confidence.is_none(),
        "a fit without an identifiability run certifies no verdict"
    );
    let fit_id = format!("{:016x}", hash.0);
    let measured = emath_lab_core::calibration::materialize_measured(&goal, &parameters, hash, confidence.as_ref())
        .expect("materialization must succeed");
    assert_eq!(measured.len(), 2, "one measured value per declared parameter");
    for (symbol, value) in &measured {
        assert!(
            (value.value - parameters.get(symbol).copied().unwrap_or(0.0)).abs() < 1e-12,
            "the measured value must be the fitted value; {symbol:?}"
        );
        assert_eq!(
            value.provenance,
            emath_ir::provenance::Provenance::Fitted {
                fit_id: fit_id.clone()
            },
            "every fitted value links to the content-addressed fit"
        );
        assert_eq!(
            value.std_uncertainty, 0.0,
            "without a verdict the uncertainty is the explicit unclaimed marker, not a claim"
        );
    }
}

#[test]
fn materialize_measured_uses_verdict_intervals() {
    let goal = fixture_goal(true);
    let data = noisy_fixture_data();
    let oracle = emath_lab_core::calibration::NumericRankOracle::default();
    let outcome = fit(&goal, &LinearModel, &data, Some(&oracle));
    let FitOutcome::Fitted {
        parameters,
        hash,
        confidence,
    } = outcome
    else {
        panic!(
            "noisy well-determined data must grant; got the outcome above"
        );
    };
    let verdict = confidence.as_ref().expect("granted fit carries its verdict");
    let fit_id = format!("{:016x}", hash.0);
    let measured = emath_lab_core::calibration::materialize_measured(&goal, &parameters, hash, Some(verdict))
        .expect("materialization must succeed");
    for (symbol, value) in &measured {
        assert_eq!(
            value.provenance,
            emath_ir::provenance::Provenance::Fitted {
                fit_id: fit_id.clone()
            }
        );
        assert!(
            value.std_uncertainty > 0.0 && value.std_uncertainty.is_finite(),
            "a certified verdict must produce a positive finite uncertainty; \
             {symbol:?}: {}",
            value.std_uncertainty
        );
        assert!(
            (value.value - parameters.get(symbol).copied().unwrap_or(0.0)).abs() < 1e-12,
            "the measured value must be the fitted value; {symbol:?}"
        );
    }
}

#[test]
fn materialize_measured_refuses_incomplete_verdict() {
    let goal = fixture_goal(false);
    let parameters = BTreeMap::from([(slope(), 2.0), (intercept(), 1.0)]);
    let hash = emath_lab_core::calibration::ProvenanceHash(0x1234);
    // Verdict covering only one of the two declared parameters.
    let verdict = emath_lab_core::calibration::Identifiability {
        directions: vec![(
            slope(),
            emath_lab_core::calibration::ConfidenceInterval {
                lo: 1.9,
                hi: 2.1,
                tight: true,
            },
        )],
    };
    assert_eq!(
        emath_lab_core::calibration::materialize_measured(&goal, &parameters, hash, Some(&verdict)),
        Err(emath_lab_core::calibration::FitMeasuredError::MissingDirection {
            name: "intercept".into()
        }),
        "a verdict missing a declared direction must refuse by name"
    );
}

#[test]
fn escalate_refuses_verdict_missing_a_declared_direction() {
    let goal = fixture_goal(true);
    let verdict = emath_lab_core::calibration::Identifiability {
        directions: vec![(
            slope(),
            ConfidenceInterval { lo: 1.9, hi: 2.1, tight: true },
        )],
    };
    assert_eq!(
        escalate(&goal, ProvenanceHash(7), Some(verdict)),
        AuthorityEscalation::Refused {
            direction: intercept(),
            reason: UnresolvedReason::SymbolicOracleUnavailable,
        },
        "a verdict that omits a declared parameter must refuse naming it — \
         no authority for unclaimed directions"
    );
}

#[test]
fn fit_refuses_unknown_or_nonpositive_weighting_and_empty_data() {
    let mut payload = pk_fixture_payload();
    payload.weights = vec![("k_el".into(), "1.0".into()), ("dose_rate".into(), "2.0".into())];
    assert_eq!(
        FitGoal::from_payload(&payload, "conc_time"),
        Err(FitPayloadError::UnknownWeightParameter {
            name: "dose_rate".into()
        }),
        "a weight naming a parameter outside the fit program must refuse"
    );
    let mut payload = pk_fixture_payload();
    payload.weights = vec![("k_el".into(), "0.0".into())];
    assert_eq!(
        FitGoal::from_payload(&payload, "conc_time"),
        Err(FitPayloadError::NonPositiveWeight {
            name: "k_el".into(),
            literal: "0.0".into()
        }),
        "a non-positive weight must refuse — weighting is never silent"
    );
    let mut payload = pk_fixture_payload();
    payload.initial = vec![("V_central".into(), "1.0".into()), ("dose_rate".into(), "2.0".into())];
    assert_eq!(
        FitGoal::from_payload(&payload, "conc_time"),
        Err(FitPayloadError::UnknownInitialParameter {
            name: "dose_rate".into()
        }),
        "a seed naming a parameter outside the fit program must refuse (never a silent drop)"
    );
    let mut payload = pk_fixture_payload();
    payload.data = vec![
        ("t".into(), Vec::<String>::new()),
        ("conc_time".into(), Vec::<String>::new()),
    ];
    assert_eq!(
        FitGoal::from_payload(&payload, "conc_time"),
        Err(FitPayloadError::EmptyData),
        "an empty coordinate row must refuse"
    );
}

#[test]
fn fit_with_no_data_rows_is_a_typed_unresolved() {
    let goal = fixture_goal(false);
    assert_eq!(
        fit(&goal, &LinearModel, &[], None),
        FitOutcome::Unresolved {
            reason: UnresolvedReason::NoData
        },
        "directly invoking the fit with zero rows must be a typed refusal, \
         never an empty-data convergence claim"
    );
}

/// A one-parameter model: `predict = p * t`.
struct OneParamModel;

impl FitModel for OneParamModel {
    fn predict(&self, parameters: &BTreeMap<SymbolId, f64>, t: f64) -> Result<f64, String> {
        Ok(parameters
            .get(&SymbolId("rate".into()))
            .copied()
            .unwrap_or(0.0)
            * t)
    }
}

#[test]
fn single_parameter_fit_round_trips_with_oracle_grant() {
    let mut goal = FitGoal::new(vec![SymbolId("rate".into())], SymbolId("response".into()));
    goal.initial = BTreeMap::from([(SymbolId("rate".into()), 0.5)]);
    goal.require_identifiability = true;
    let data = vec![
        FitRow { t: 1.0, y: 3.0, weight: 1.0 },
        FitRow { t: 2.0, y: 6.0, weight: 1.0 },
        FitRow { t: 4.0, y: 12.0, weight: 1.0 },
    ];
    let oracle = emath_lab_core::calibration::NumericRankOracle::default();
    let FitOutcome::Fitted {
        parameters,
        confidence,
        ..
    } = fit(&goal, &OneParamModel, &data, Some(&oracle))
    else {
        panic!("a single well-determined parameter must grant; got the outcome above");
    };
    assert!(
        (parameters.get(&SymbolId("rate".into())).copied().unwrap_or(0.0) - 3.0).abs() < 1e-4,
        "rate must converge to 3.0"
    );
    let verdict = confidence.expect("granted fit carries its verdict");
    assert_eq!(verdict.directions.len(), 1);
    assert!(
        verdict.directions[0].1.tight,
        "the single direction must be tight"
    );
}

#[test]
fn numeric_rank_oracle_refuses_zero_valued_direction() {
    // Data on `y = 2t + 0` with noise: the intercept is well-determined
    // but its value sits AT zero — the data cannot certify the SIGN, so
    // the direction is not tight and escalation must refuse naming it.
    let goal = fixture_goal(true);
    let data = vec![
        FitRow { t: 0.0, y: 0.02, weight: 1.0 },
        FitRow { t: 1.0, y: 2.01, weight: 1.0 },
        FitRow { t: 2.0, y: 3.98, weight: 1.0 },
        FitRow { t: 3.0, y: 6.03, weight: 1.0 },
    ];
    let oracle = emath_lab_core::calibration::NumericRankOracle::default();
    let outcome = fit(&goal, &LinearModel, &data, Some(&oracle));
    assert!(
        matches!(
            outcome,
            FitOutcome::AuthorityRefused {
                ref direction,
                reason: UnresolvedReason::StructureNotIdentifiable,
            } if *direction == intercept()
        ),
        "a zero-valued direction whose interval straddles zero must refuse \
         escalation; got: {outcome:?}"
    );
}

/// A model that refuses to evaluate (domain error) — the fit must
/// refuse, never optimize over poisoned values.
struct FailingModel;

impl FitModel for FailingModel {
    fn predict(&self, _parameters: &BTreeMap<SymbolId, f64>, _t: f64) -> Result<f64, String> {
        Err("sqrt of negative concentration".to_string())
    }
}

#[test]
fn fit_refuses_model_evaluation_faults() {
    let goal = fixture_goal(false);
    let data = fixture_data();
    assert_eq!(
        fit(&goal, &FailingModel, &data, None),
        FitOutcome::ModelError {
            detail: "sqrt of negative concentration".into()
        },
        "a model fault must surface as the typed ModelError disposition, \
         never NaN-poisoned parameters"
    );
}

#[test]
fn fit_values_are_invariant_under_row_permutation() {
    let goal = fixture_goal(false);
    let data = fixture_data();
    let permuted = vec![
        data[3].clone(),
        data[1].clone(),
        data[0].clone(),
        data[2].clone(),
    ];
    let FitOutcome::Fitted { parameters: a, .. } = fit(&goal, &LinearModel, &data, None) else {
        panic!("original data must fit");
    };
    let FitOutcome::Fitted { parameters: b, .. } = fit(&goal, &LinearModel, &permuted, None)
    else {
        panic!("permuted data must fit");
    };
    assert!(
        (a.get(&slope()).copied().unwrap_or(0.0) - b.get(&slope()).copied().unwrap_or(0.0))
            .abs()
            < 1e-9
            && (a.get(&intercept()).copied().unwrap_or(0.0)
                - b.get(&intercept()).copied().unwrap_or(0.0))
            .abs()
                < 1e-9,
        "fitted values are invariant under row permutation"
    );
}

#[test]
fn fit_values_scale_with_uniform_data_rescaling() {
    let goal = fixture_goal(false);
    let scale = 7.5;
    let scaled: Vec<FitRow> = fixture_data()
        .iter()
        .map(|row| FitRow { t: row.t, y: row.y * scale, weight: row.weight })
        .collect();
    let FitOutcome::Fitted { parameters: base, .. } = fit(&goal, &LinearModel, &fixture_data(), None)
    else {
        panic!("base data must fit");
    };
    let FitOutcome::Fitted { parameters: rescaled, .. } = fit(&goal, &LinearModel, &scaled, None)
    else {
        panic!("rescaled data must fit");
    };
    for (name, base_value) in &base {
        let rescaled_value = rescaled.get(name).copied().unwrap_or(0.0);
        assert!(
            (rescaled_value - base_value * scale).abs() < 1e-6,
            "fitted values must scale linearly with uniform data rescaling; \
             {name:?}: {rescaled_value} vs {base_value} * {scale}"
        );
    }
}

#[test]
fn fit_values_are_invariant_under_uniform_row_weight_scaling() {
    let goal = fixture_goal(false);
    let scaled: Vec<FitRow> = fixture_data()
        .iter()
        .map(|row| FitRow { t: row.t, y: row.y, weight: row.weight * 7.0 })
        .collect();
    let FitOutcome::Fitted { parameters: base, .. } = fit(&goal, &LinearModel, &fixture_data(), None)
    else {
        panic!("base data must fit");
    };
    let FitOutcome::Fitted { parameters: weighted, .. } = fit(&goal, &LinearModel, &scaled, None)
    else {
        panic!("weight-scaled data must fit");
    };
    for (name, base_value) in &base {
        let weighted_value = weighted.get(name).copied().unwrap_or(0.0);
        assert!(
            (weighted_value - base_value).abs() < 1e-9,
            "uniform row-weight scaling must not move the fit; {name:?}: \
             {weighted_value} vs {base_value}"
        );
    }
}

/// Failure-first (review pass): the numeric rank oracle must resolve a
/// FULL-RANK but ill-conditioned design as identifiable. The design
/// here has eigenvalue ratio `lambda_min / lambda_max ~ 6.7e-9`, above
/// the default `rel_tolerance` of 1e-9, so both directions are
/// identifiable and the exact-fit residuals make every interval tight.
/// A Jacobian eigensolver that is not a true similarity transform
/// (column rotation with premature symmetric mirroring corrupts the
/// not-yet-processed entries) distorts the small eigenvalue by O(1e-4)
/// and misclassifies the rank, reporting every direction relaxed.
#[test]
fn rank_oracle_resolves_ill_conditioned_full_rank_design() {
    use emath_lab_core::calibration::NumericRankOracle;

    // t = 1, 1+2e-4, 1+4e-4: columns [t, 1] are nearly collinear but
    // J^T J keeps full rank (det = 6*(2e-4)^2 = 2.4e-7).
    let ts = [1.0_f64, 1.0002, 1.0004];
    let slope_value = 0.5_f64;
    let intercept_value = 2.0_f64;
    let data: Vec<FitRow> = ts
        .iter()
        .map(|t| FitRow {
            t: *t,
            y: slope_value * t + intercept_value,
            weight: 1.0,
        })
        .collect();
    let fitted = BTreeMap::from([(slope(), slope_value), (intercept(), intercept_value)]);
    let goal = FitGoal::new(vec![slope(), intercept()], SymbolId("response".into()));

    let oracle = NumericRankOracle::default();
    let verdict = oracle
        .structural_identifiability(&goal, &LinearModel, &data, &fitted)
        .expect("3 rows > 2 parameters: the oracle must serve a verdict");
    assert_eq!(verdict.directions.len(), 2, "one direction per parameter");
    for (symbol, interval) in &verdict.directions {
        assert!(
            interval.tight,
            "full-rank design must certify {symbol:?} as tight, got {interval:?}"
        );
        assert!(
            interval.lo.is_finite() && interval.hi.is_finite(),
            "tight intervals are finite; {symbol:?}: {interval:?}"
        );
    }
}

/// Failure-first (review pass): a rank-DEFICIENT design (all `t` equal —
/// slope and intercept are indistinguishable) must report every
/// direction RELAXED through a served verdict (`Some`), so escalation
/// refuses with `StructureNotIdentifiable` — never with the
/// oracle-unavailable reason, which blames the missing provider instead
/// of the data. An eigensolver that is not a true similarity transform
/// corrupts the spectrum of `[[12,6],[6,3]]` into two positive
/// diagonals, misclassifies the rank as full, and then falls through to
/// the singular-inverse `None` path.
#[test]
fn rank_oracle_reports_relaxed_directions_for_rank_deficient_design() {
    use emath_lab_core::calibration::NumericRankOracle;

    // All `t` equal: the [t, 1] design columns are proportional, so
    // J^T J = [[12, 6], [6, 3]] has rank 1 and a zero eigenvalue.
    let data: Vec<FitRow> = [2.0_f64; 3]
        .iter()
        .map(|t| FitRow { t: *t, y: 3.0, weight: 1.0 })
        .collect();
    let fitted = BTreeMap::from([(slope(), 0.5), (intercept(), 2.0)]);
    let goal = FitGoal::new(vec![slope(), intercept()], SymbolId("response".into()));

    let oracle = NumericRankOracle::default();
    let verdict = oracle
        .structural_identifiability(&goal, &LinearModel, &data, &fitted)
        .expect("rank-deficient design must serve a verdict with relaxed directions");
    assert_eq!(verdict.directions.len(), 2, "one direction per parameter");
    for (symbol, interval) in &verdict.directions {
        assert!(
            !interval.tight,
            "rank-deficient design must report {symbol:?} relaxed, got {interval:?}"
        );
        assert_eq!(
            (interval.lo, interval.hi),
            (f64::NEG_INFINITY, f64::INFINITY),
            "relaxed intervals are the full real line; {symbol:?}: {interval:?}"
        );
    }
}

/// A model whose domain excludes part of parameter space: `a < 0`
/// produces NaN (the value is computed, not an `Err` — the fit must
/// still refuse).
struct NegativeDomainModel;

impl FitModel for NegativeDomainModel {
    fn predict(&self, parameters: &BTreeMap<SymbolId, f64>, _t: f64) -> Result<f64, String> {
        let a = parameters
            .get(&SymbolId("a".into()))
            .copied()
            .unwrap_or(0.0);
        if a < 0.0 {
            Ok(f64::NAN)
        } else {
            Ok(a)
        }
    }
}

/// Failure-first (review pass): a model that returns non-finite values
/// (computed NaN, not an `Err`) must produce `ModelError` — the fit
/// refuses, it never returns a NaN-poisoned fit. The documented
/// contract ("the fit refuses, it never NaN-poisons the optimizer")
/// covered only `Err` results; a NaN `Ok` silently flowed through 256
/// reject iterations and returned `Fitted` with the NaN seed.
#[test]
fn fit_refuses_non_finite_model_output_instead_of_returning_nan() {
    let goal = FitGoal::new(vec![SymbolId("a".into())], SymbolId("y".into()));
    goal.initial.iter().next();
    let mut goal = goal;
    goal.initial.insert(SymbolId("a".into()), -1.0);
    let data = vec![FitRow { t: 0.0, y: 4.0, weight: 1.0 }];
    let outcome = fit(&goal, &NegativeDomainModel, &data, None);
    assert!(
        matches!(outcome, FitOutcome::ModelError { .. }),
        "a non-finite SSE must refuse as ModelError, got {outcome:?}"
    );
}

/// A model that is finite at the evaluated point but non-finite at the
/// ±1e-6 finite-difference offsets: the JACOBIAN is poisoned while the
/// residuals stay finite. The fit must refuse (`ModelError`), never
/// silently stall on the seed for 256 rejected iterations and return
/// the seed as a "fit".
struct EdgeDomainModel;

impl FitModel for EdgeDomainModel {
    fn predict(&self, parameters: &BTreeMap<SymbolId, f64>, _t: f64) -> Result<f64, String> {
        let a = parameters
            .get(&SymbolId("a".into()))
            .copied()
            .unwrap_or(0.0);
        if a < 0.0 {
            Ok(f64::NAN)
        } else {
            Ok(a.sqrt())
        }
    }
}

#[test]
fn fit_refuses_non_finite_jacobian_entries() {
    let mut goal = FitGoal::new(vec![SymbolId("a".into())], SymbolId("y".into()));
    goal.initial.insert(SymbolId("a".into()), 0.0);
    let data = vec![FitRow { t: 0.0, y: 1.0, weight: 1.0 }];
    let outcome = fit(&goal, &EdgeDomainModel, &data, None);
    assert!(
        matches!(outcome, FitOutcome::ModelError { .. }),
        "a non-finite Jacobian must refuse as ModelError, got {outcome:?}"
    );
}

/// Failure-first (review pass): two data rows naming the observable must
/// refuse (`DuplicateObservableRow`), never silently take the last one —
/// the single-coordinate rule refuses `TooManyCoordinateRows`, and the
/// observable row deserves the same strictness.
#[test]
fn duplicate_observable_data_row_refuses() {
    let mut payload = pk_fixture_payload();
    payload
        .data
        .push(("conc_time".into(), vec!["9.0".into(); 4]));
    let error = FitGoal::from_payload(&payload, "conc_time")
        .expect_err("a second observable row must refuse, not silently replace the first");
    assert!(
        matches!(error, FitPayloadError::DuplicateObservableRow { .. }),
        "got {error:?}"
    );
}

/// Failure-first (review pass): the provenance hash must distinguish
/// fit programs that differ ONLY in their declared per-parameter
/// weights. The weights steer the Jacobian (and thus the fit), so two
/// programs with different declared weights are different programs even
/// when they converge to the same optimum — the content-addressed hash
/// must not collide.
#[test]
fn provenance_hash_distinguishes_declared_weights() {
    use emath_lab_core::calibration::provenance;

    let plain = fixture_goal(false);
    let mut weighted = plain.clone();
    weighted.weights = ResidualWeights(BTreeMap::from([(slope(), 2.0)]));
    let data = fixture_data();
    let fitted = BTreeMap::from([(slope(), 2.0), (intercept(), 1.0)]);
    assert_ne!(
        provenance(&plain, &data, &fitted),
        provenance(&weighted, &data, &fitted),
        "declared weights are part of the fit program and must enter the provenance hash"
    );
}
