//! Failure-first generic fit-goal runtime tests (04 section 5.3).

use std::collections::BTreeMap;

use emath_ir::goal::GoalPayload;
use emath_lab_core::calibration::{
    AuthorityEscalation, ConfidenceInterval, FitGoal, FitModel, FitOutcome, FitPayloadError,
    FitRow, Identifiability, IdentifiabilityProvider, OptimizerMethod, ProvenanceHash,
    ResidualMethod, ResidualWeights, UnresolvedReason, escalate, fit, jacobian_residuals,
    weighted_residuals,
};
use emath_term::SymbolId;
use emath_test_harness::Probe;

struct LinearModel;

impl FitModel for LinearModel {
    fn predict(&self, parameters: &BTreeMap<SymbolId, f64>, t: f64) -> Result<f64, String> {
        let slope = parameters.get(&SymbolId("slope".into())).copied().unwrap_or(0.0);
        let intercept = parameters.get(&SymbolId("intercept".into())).copied().unwrap_or(0.0);
        Ok(slope * t + intercept)
    }
}

struct SumModel;

impl FitModel for SumModel {
    fn predict(&self, parameters: &BTreeMap<SymbolId, f64>, _t: f64) -> Result<f64, String> {
        Ok(parameters.get(&slope()).copied().unwrap_or(0.0) + parameters.get(&intercept()).copied().unwrap_or(0.0))
    }
}

struct OneParamModel;

impl FitModel for OneParamModel {
    fn predict(&self, parameters: &BTreeMap<SymbolId, f64>, t: f64) -> Result<f64, String> {
        Ok(parameters.get(&SymbolId("rate".into())).copied().unwrap_or(0.0) * t)
    }
}

struct FailingModel;

impl FitModel for FailingModel {
    fn predict(&self, _parameters: &BTreeMap<SymbolId, f64>, _t: f64) -> Result<f64, String> {
        Err("sqrt of negative concentration".to_string())
    }
}

struct NegativeDomainModel;

impl FitModel for NegativeDomainModel {
    fn predict(&self, parameters: &BTreeMap<SymbolId, f64>, _t: f64) -> Result<f64, String> {
        let a = parameters.get(&SymbolId("a".into())).copied().unwrap_or(0.0);
        if a < 0.0 { Ok(f64::NAN) } else { Ok(a) }
    }
}

struct EdgeDomainModel;

impl FitModel for EdgeDomainModel {
    fn predict(&self, parameters: &BTreeMap<SymbolId, f64>, _t: f64) -> Result<f64, String> {
        let a = parameters.get(&SymbolId("a".into())).copied().unwrap_or(0.0);
        if a < 0.0 { Ok(f64::NAN) } else { Ok(a.sqrt()) }
    }
}

struct LooseIntercept;
impl IdentifiabilityProvider for LooseIntercept {
    fn structural_identifiability(&self, goal: &FitGoal, _model: &dyn FitModel, _data: &[FitRow], _fitted: &BTreeMap<SymbolId, f64>) -> Option<Identifiability> {
        Some(Identifiability { directions: vec![(goal.parameters[0].clone(), ConfidenceInterval { lo: -0.01, hi: 0.01, tight: true }), (goal.parameters[1].clone(), ConfidenceInterval { lo: -5.0, hi: 5.0, tight: false })] })
    }
}

struct AllTight;
impl IdentifiabilityProvider for AllTight {
    fn structural_identifiability(&self, goal: &FitGoal, _model: &dyn FitModel, _data: &[FitRow], _fitted: &BTreeMap<SymbolId, f64>) -> Option<Identifiability> {
        Some(Identifiability { directions: goal.parameters.iter().cloned().map(|s| (s, ConfidenceInterval { lo: -0.01, hi: 0.01, tight: true })).collect() })
    }
}

fn slope() -> SymbolId {
    SymbolId("slope".into())
}

fn intercept() -> SymbolId {
    SymbolId("intercept".into())
}

fn fixture_goal(require_identifiability: bool) -> FitGoal {
    let mut goal = FitGoal::new(vec![slope(), intercept()], SymbolId("response".into()));
    goal.model = vec!["LinearModel".into()];
    goal.prediction = "value".into();
    goal.weights = ResidualWeights(BTreeMap::from([(slope(), 1.0), (intercept(), 1.0)]));
    goal.initial = BTreeMap::from([(slope(), 1.0), (intercept(), 0.0)]);
    goal.require_identifiability = require_identifiability;
    goal
}

fn fixture_data() -> Vec<FitRow> {
    vec![FitRow { t: 0.0, y: 1.0, weight: 1.0 }, FitRow { t: 1.0, y: 3.0, weight: 1.0 }, FitRow { t: 2.0, y: 5.0, weight: 1.0 }, FitRow { t: 3.0, y: 7.0, weight: 1.0 }]
}

fn noisy_fixture_data() -> Vec<FitRow> {
    vec![FitRow { t: 0.0, y: 1.01, weight: 1.0 }, FitRow { t: 1.0, y: 3.02, weight: 1.0 }, FitRow { t: 2.0, y: 4.99, weight: 1.0 }, FitRow { t: 3.0, y: 7.0, weight: 1.0 }]
}

fn pk_fixture_payload() -> GoalPayload {
    let mut payload: GoalPayload = Default::default();
    payload.parameters = vec!["k_el".into(), "V_central".into()];
    payload.model = vec!["PK_TwoCompartment".into()];
    payload.prediction = "central".into();
    payload.residual = "weighted_least_squares".into();
    payload.method = "levenberg_marquardt".into();
    payload.initial = vec![("k_el".into(), "0.2".into()), ("V_central".into(), "1.0".into())];
    payload.weights = vec![("k_el".into(), "1.0".into()), ("V_central".into(), "1.0".into())];
    payload.data = vec![("t".into(), vec!["0.5".into(), "1.0".into(), "2.0".into(), "4.0".into()]), ("conc_time".into(), vec!["2.41".into(), "1.93".into(), "1.24".into(), "0.64".into()])];
    payload.require_identifiability = true;
    payload
}

fn fitted_params(p: &mut Probe, outcome: &FitOutcome, name: &str) -> Option<BTreeMap<SymbolId, f64>> {
    match outcome {
        FitOutcome::Fitted { parameters, .. } => Some(parameters.clone()),
        other => {
            p.fail(name, format!("must materialize Fitted; got {other:?}"));
            None
        }
    }
}

#[test]
fn fit_goal_runtime() {
    let mut p = Probe::new("the generic fit program fits, proves provenance, and refuses honestly");
    p.case("converges-with-provenance", |p| {
        let goal = fixture_goal(false);
        let data = fixture_data();
        let first = fit(&goal, &LinearModel, &data, None);
        let (fitted_slope, fitted_intercept, hash) = match &first {
            FitOutcome::Fitted { parameters, hash, .. } => (parameters.get(&slope()).copied().unwrap_or(0.0), parameters.get(&intercept()).copied().unwrap_or(0.0), hash.0),
            other => {
                p.fail("fitted", format!("generic LM fit must materialize Fitted; got {other:?}"));
                return;
            }
        };
        p.ne("hash-nonzero", hash, 0);
        p.close("slope", fitted_slope, 2.0, 1e-4);
        p.close("intercept", fitted_intercept, 1.0, 1e-4);
        match fit(&goal, &LinearModel, &data, None) {
            FitOutcome::Fitted { hash: second, .. } => p.eq("deterministic", hash, second.0),
            other => p.fail("second", format!("second fit must also be Fitted; got {other:?}")),
        }
    });
    p.case("unresolved-without-provider", |p| {
        let outcome = fit(&fixture_goal(true), &LinearModel, &fixture_data(), None);
        p.demand("honest", matches!(outcome, FitOutcome::Unresolved { reason: UnresolvedReason::SymbolicOracleUnavailable }), format!("must stay honestly unresolved; got {outcome:?}"));
    });
    p.case("relaxed-direction-refused", |p| {
        let outcome = fit(&fixture_goal(true), &LinearModel, &fixture_data(), Some(&LooseIntercept));
        p.demand("names-intercept", matches!(outcome, FitOutcome::AuthorityRefused { ref direction, reason: UnresolvedReason::StructureNotIdentifiable } if *direction == intercept()), format!("relaxed direction must refuse escalation; got {outcome:?}"));
    });
    p.case("all-tight-grants", |p| {
        let goal = fixture_goal(true);
        let outcome = fit(&goal, &LinearModel, &fixture_data(), Some(&AllTight));
        p.demand("grants", matches!(outcome, FitOutcome::Fitted { .. }), format!("all-tight directions grant escalation; got {outcome:?}"));
        p.demand("no-verdict-no-grant", matches!(escalate(&goal, ProvenanceHash(7), None), AuthorityEscalation::Refused { ref direction, reason: UnresolvedReason::SymbolicOracleUnavailable } if *direction == slope()), "missing verdict refuses naming the first parameter");
    });
    p.case("weights-scale-jacobian", |p| {
        let goal = fixture_goal(false);
        let data = fixture_data();
        let residuals = weighted_residuals(&goal, &LinearModel, &data, &goal.initial).expect("linear model");
        let expected: Vec<f64> = data.iter().map(|row| row.weight * (1.0 * row.t - row.y)).collect();
        p.eq("residuals", residuals, expected);
        let mut weighted = goal.clone();
        weighted.weights = ResidualWeights(BTreeMap::from([(slope(), 2.0), (intercept(), 1.0)]));
        let plain = jacobian_residuals(&goal, &LinearModel, &data, &goal.initial).expect("linear model");
        let scaled = jacobian_residuals(&weighted, &LinearModel, &data, &goal.initial).expect("linear model");
        for row in 0..data.len() {
            let col = row * goal.parameters.len();
            p.close(format!("slope-col-{row}"), scaled[col], 2.0 * plain[col], 1e-9);
            p.eq(format!("intercept-col-{row}"), scaled[col + 1], plain[col + 1]);
        }
        p.eq("residual-spelling", goal.residual, ResidualMethod::WeightedLeastSquares);
        p.eq("method-spelling", goal.method, OptimizerMethod::LevenbergMarquardt);
        p.eq("residual-str", goal.residual.as_str(), "weighted_least_squares");
        p.eq("method-str", goal.method.as_str(), "levenberg_marquardt");
    });
    p.case("payload-traces-losslessly", |p| {
        let payload = pk_fixture_payload();
        let goal = FitGoal::from_payload(&payload, "conc_time").expect("payload must trace");
        p.eq("params", goal.parameters, vec![SymbolId("k_el".into()), SymbolId("V_central".into())]);
        p.eq("observable", goal.observable, SymbolId("conc_time".into()));
        p.eq("model", goal.model, vec!["PK_TwoCompartment"]);
        p.eq("prediction", goal.prediction, "central".to_string());
        p.eq("residual", goal.residual, ResidualMethod::WeightedLeastSquares);
        p.eq("method", goal.method, OptimizerMethod::LevenbergMarquardt);
        p.eq("initial", goal.initial, BTreeMap::from([(SymbolId("k_el".into()), 0.2), (SymbolId("V_central".into()), 1.0)]));
        p.eq("weights", goal.weights.0, BTreeMap::from([(SymbolId("k_el".into()), 1.0), (SymbolId("V_central".into()), 1.0)]));
        p.eq("data", goal.data, vec![FitRow { t: 0.5, y: 2.41, weight: 1.0 }, FitRow { t: 1.0, y: 1.93, weight: 1.0 }, FitRow { t: 2.0, y: 1.24, weight: 1.0 }, FitRow { t: 4.0, y: 0.64, weight: 1.0 }]);
        p.eq("coordinate", goal.coordinate, "t".to_string());
        p.demand("gate", goal.require_identifiability, "honesty gate must survive");
    });
    p.case("traced-program-runs", |p| {
        let payload = pk_fixture_payload();
        let mut goal = FitGoal::from_payload(&payload, "conc_time").expect("payload must trace");
        goal.parameters = vec![slope(), intercept()];
        goal.initial = BTreeMap::from([(slope(), 1.0), (intercept(), 0.0)]);
        goal.weights = ResidualWeights(BTreeMap::from([(slope(), 1.0), (intercept(), 1.0)]));
        goal.data = fixture_data();
        goal.require_identifiability = false;
        let outcome = fit(&goal, &LinearModel, &goal.data.clone(), None);
        let Some(parameters) = fitted_params(p, &outcome, "traced") else { return; };
        p.close("slope", parameters.get(&slope()).copied().unwrap_or(0.0), 2.0, 1e-4);
        p.close("intercept", parameters.get(&intercept()).copied().unwrap_or(0.0), 1.0, 1e-4);
    });
    p.case("payload-refusals", |p| {
        let mut payload = pk_fixture_payload();
        payload.method = "firefly".into();
        p.eq("method", FitGoal::from_payload(&payload, "conc_time"), Err(FitPayloadError::UnknownMethod("firefly".into())));
        let mut payload = pk_fixture_payload();
        payload.initial = vec![("k_el".into(), "abc".into())];
        p.eq("number", FitGoal::from_payload(&payload, "conc_time"), Err(FitPayloadError::UnparseableNumber { row: "initial".into(), name: "k_el".into(), literal: "abc".into() }));
        let mut payload = pk_fixture_payload();
        payload.parameters.clear();
        p.eq("params", FitGoal::from_payload(&payload, "conc_time"), Err(FitPayloadError::MissingParameters));
        let mut payload = pk_fixture_payload();
        payload.residual.clear();
        p.eq("residual", FitGoal::from_payload(&payload, "conc_time"), Err(FitPayloadError::MissingResidual));
        let mut payload = pk_fixture_payload();
        payload.data.clear();
        p.eq("data", FitGoal::from_payload(&payload, "conc_time"), Err(FitPayloadError::MissingData));
        let mut payload = pk_fixture_payload();
        payload.data = vec![("t".into(), vec!["0.5".into(), "1.0".into()])];
        p.eq("observable-row", FitGoal::from_payload(&payload, "conc_time"), Err(FitPayloadError::MissingObservableRow { observable: "conc_time".into() }));
        let mut payload = pk_fixture_payload();
        payload.data = vec![("t".into(), vec!["0.5".into(), "1.0".into()]), ("conc_time".into(), vec!["2.41".into(), "1.93".into()]), ("extra".into(), vec!["1.0".into()])];
        p.eq("coordinate-rows", FitGoal::from_payload(&payload, "conc_time"), Err(FitPayloadError::TooManyCoordinateRows { observable: "conc_time".into(), extra: "extra".into() }));
        let mut payload = pk_fixture_payload();
        payload.data = vec![("t".into(), vec!["0.5".into(), "1.0".into(), "2.0".into()]), ("conc_time".into(), vec!["2.41".into(), "1.93".into()])];
        p.eq("arity", FitGoal::from_payload(&payload, "conc_time"), Err(FitPayloadError::DataLengthMismatch { coordinate: "t".into(), coordinate_len: 3, observable: "conc_time".into(), observable_len: 2 }));
        let mut payload = pk_fixture_payload();
        payload.data = vec![("t".into(), vec!["0.5".into(), "abc".into()]), ("conc_time".into(), vec!["2.41".into(), "1.93".into()])];
        p.eq("data-number", FitGoal::from_payload(&payload, "conc_time"), Err(FitPayloadError::UnparseableNumber { row: "data:t".into(), name: "t".into(), literal: "abc".into() }));
    });
    p.case("rank-oracle-full-rank", |p| {
        let goal = fixture_goal(true);
        let oracle = emath_lab_core::calibration::NumericRankOracle::default();
        let outcome = fit(&goal, &LinearModel, &fixture_data(), Some(&oracle));
        let (parameters, verdict) = match &outcome {
            FitOutcome::Fitted { parameters, confidence, .. } => (parameters, confidence.as_ref().expect("granted fit carries its verdict")),
            other => {
                p.fail("grants", format!("full-rank exact data must grant escalation; got {other:?}"));
                return;
            }
        };
        p.close("slope", parameters.get(&slope()).copied().unwrap_or(0.0), 2.0, 1e-4);
        p.close("intercept", parameters.get(&intercept()).copied().unwrap_or(0.0), 1.0, 1e-4);
        p.eq("directions", verdict.directions.len(), 2);
        for (symbol, interval) in &verdict.directions {
            p.demand(format!("tight-{symbol:?}"), interval.tight && interval.lo.is_finite() && interval.hi.is_finite(), format!("full-rank directions tight and finite; {symbol:?}: {interval:?}"));
        }
    });
    p.case("rank-oracle-collinear", |p| {
        let goal = fixture_goal(true);
        let data = vec![FitRow { t: 0.0, y: 3.0, weight: 1.0 }, FitRow { t: 1.0, y: 3.1, weight: 1.0 }, FitRow { t: 2.0, y: 2.9, weight: 1.0 }];
        let oracle = emath_lab_core::calibration::NumericRankOracle::default();
        let outcome = fit(&goal, &SumModel, &data, Some(&oracle));
        p.demand("names-slope", matches!(outcome, FitOutcome::AuthorityRefused { ref direction, reason: UnresolvedReason::StructureNotIdentifiable } if *direction == slope()), format!("rank-deficient Jacobian must refuse naming slope; got {outcome:?}"));
    });
    p.case("rank-oracle-underdetermined", |p| {
        let goal = fixture_goal(true);
        let data = vec![FitRow { t: 0.0, y: 1.0, weight: 1.0 }];
        let oracle = emath_lab_core::calibration::NumericRankOracle::default();
        let outcome = fit(&goal, &LinearModel, &data, Some(&oracle));
        p.demand("honest-reason", matches!(outcome, FitOutcome::AuthorityRefused { ref direction, reason: UnresolvedReason::SymbolicOracleUnavailable } if *direction == slope()), format!("uncertifiable data refuses with the honest reason; got {outcome:?}"));
    });
    p.case("measured-links-hash", |p| {
        let goal = fixture_goal(false);
        let data = noisy_fixture_data();
        let (parameters, hash, confidence) = match fit(&goal, &LinearModel, &data, None) {
            FitOutcome::Fitted { parameters, hash, confidence } => (parameters, hash, confidence),
            other => {
                p.fail("converges", format!("fit must converge on noisy data; got {other:?}"));
                return;
            }
        };
        p.demand("no-verdict", confidence.is_none(), "fit without identifiability certifies no verdict");
        let fit_id = format!("{:016x}", hash.0);
        let measured = emath_lab_core::calibration::materialize_measured(&goal, &parameters, hash, confidence.as_ref()).expect("materialization must succeed");
        p.eq("count", measured.len(), 2);
        for (symbol, value) in &measured {
            p.close(format!("value-{symbol:?}"), value.value, parameters.get(symbol).copied().unwrap_or(0.0), 1e-12);
            p.eq(format!("link-{symbol:?}"), value.provenance, emath_ir::provenance::Provenance::Fitted { fit_id: fit_id.clone() });
            p.eq(format!("uncertain-{symbol:?}"), value.std_uncertainty, 0.0);
        }
    });
    p.case("measured-uses-verdict", |p| {
        let goal = fixture_goal(true);
        let oracle = emath_lab_core::calibration::NumericRankOracle::default();
        let outcome = fit(&goal, &LinearModel, &noisy_fixture_data(), Some(&oracle));
        let (parameters, hash, verdict) = match outcome {
            FitOutcome::Fitted { parameters, hash, confidence } => {
                let verdict = confidence.expect("granted fit carries its verdict");
                (parameters, hash, verdict)
            }
            other => {
                p.fail("grants", format!("noisy well-determined data must grant; got {other:?}"));
                return;
            }
        };
        let fit_id = format!("{:016x}", hash.0);
        let measured = emath_lab_core::calibration::materialize_measured(&goal, &parameters, hash, Some(&verdict)).expect("materialization must succeed");
        for (symbol, value) in &measured {
            p.eq(format!("link-{symbol:?}"), value.provenance, emath_ir::provenance::Provenance::Fitted { fit_id: fit_id.clone() });
            p.demand(format!("uncertain-{symbol:?}"), value.std_uncertainty > 0.0 && value.std_uncertainty.is_finite(), format!("certified verdict gives positive finite uncertainty; {symbol:?}: {}", value.std_uncertainty));
            p.close(format!("value-{symbol:?}"), value.value, parameters.get(symbol).copied().unwrap_or(0.0), 1e-12);
        }
    });
    p.case("measured-incomplete-refused", |p| {
        let goal = fixture_goal(false);
        let parameters = BTreeMap::from([(slope(), 2.0), (intercept(), 1.0)]);
        let verdict = emath_lab_core::calibration::Identifiability { directions: vec![(slope(), emath_lab_core::calibration::ConfidenceInterval { lo: 1.9, hi: 2.1, tight: true })] };
        p.eq("missing-direction", emath_lab_core::calibration::materialize_measured(&goal, &parameters, emath_lab_core::calibration::ProvenanceHash(0x1234), Some(&verdict)), Err(emath_lab_core::calibration::FitMeasuredError::MissingDirection { name: "intercept".into() }));
    });
    p.case("escalate-missing-direction", |p| {
        let goal = fixture_goal(true);
        let verdict = emath_lab_core::calibration::Identifiability { directions: vec![(slope(), ConfidenceInterval { lo: 1.9, hi: 2.1, tight: true })] };
        p.eq("names-intercept", escalate(&goal, ProvenanceHash(7), Some(verdict)), AuthorityEscalation::Refused { direction: intercept(), reason: UnresolvedReason::SymbolicOracleUnavailable });
    });
    p.case("weight-seed-data-refusals", |p| {
        let mut payload = pk_fixture_payload();
        payload.weights = vec![("k_el".into(), "1.0".into()), ("dose_rate".into(), "2.0".into())];
        p.eq("weight-param", FitGoal::from_payload(&payload, "conc_time"), Err(FitPayloadError::UnknownWeightParameter { name: "dose_rate".into() }));
        let mut payload = pk_fixture_payload();
        payload.weights = vec![("k_el".into(), "0.0".into())];
        p.eq("weight-sign", FitGoal::from_payload(&payload, "conc_time"), Err(FitPayloadError::NonPositiveWeight { name: "k_el".into(), literal: "0.0".into() }));
        let mut payload = pk_fixture_payload();
        payload.initial = vec![("V_central".into(), "1.0".into()), ("dose_rate".into(), "2.0".into())];
        p.eq("seed-param", FitGoal::from_payload(&payload, "conc_time"), Err(FitPayloadError::UnknownInitialParameter { name: "dose_rate".into() }));
        let mut payload = pk_fixture_payload();
        payload.data = vec![("t".into(), Vec::<String>::new()), ("conc_time".into(), Vec::<String>::new())];
        p.eq("empty-data", FitGoal::from_payload(&payload, "conc_time"), Err(FitPayloadError::EmptyData));
    });
    p.case("no-rows-unresolved", |p| {
        p.eq("typed", fit(&fixture_goal(false), &LinearModel, &[], None), FitOutcome::Unresolved { reason: UnresolvedReason::NoData });
    });
    p.case("single-param-grant", |p| {
        let mut goal = FitGoal::new(vec![SymbolId("rate".into())], SymbolId("response".into()));
        goal.initial = BTreeMap::from([(SymbolId("rate".into()), 0.5)]);
        goal.require_identifiability = true;
        let data = vec![FitRow { t: 1.0, y: 3.0, weight: 1.0 }, FitRow { t: 2.0, y: 6.0, weight: 1.0 }, FitRow { t: 4.0, y: 12.0, weight: 1.0 }];
        let oracle = emath_lab_core::calibration::NumericRankOracle::default();
        let (parameters, verdict) = match fit(&goal, &OneParamModel, &data, Some(&oracle)) {
            FitOutcome::Fitted { parameters, confidence, .. } => (parameters, confidence.expect("granted fit carries its verdict")),
            other => {
                p.fail("grants", format!("single well-determined parameter must grant; got {other:?}"));
                return;
            }
        };
        p.close("rate", parameters.get(&SymbolId("rate".into())).copied().unwrap_or(0.0), 3.0, 1e-4);
        p.eq("directions", verdict.directions.len(), 1);
        p.demand("tight", verdict.directions[0].1.tight, "single direction must be tight");
    });
    p.case("zero-direction-refused", |p| {
        let goal = fixture_goal(true);
        let data = vec![FitRow { t: 0.0, y: 0.02, weight: 1.0 }, FitRow { t: 1.0, y: 2.01, weight: 1.0 }, FitRow { t: 2.0, y: 3.98, weight: 1.0 }, FitRow { t: 3.0, y: 6.03, weight: 1.0 }];
        let oracle = emath_lab_core::calibration::NumericRankOracle::default();
        let outcome = fit(&goal, &LinearModel, &data, Some(&oracle));
        p.demand("names-intercept", matches!(outcome, FitOutcome::AuthorityRefused { ref direction, reason: UnresolvedReason::StructureNotIdentifiable } if *direction == intercept()), format!("zero-valued direction must refuse escalation; got {outcome:?}"));
    });
    p.case("model-fault-refused", |p| {
        p.eq("fault", fit(&fixture_goal(false), &FailingModel, &fixture_data(), None), FitOutcome::ModelError { detail: "sqrt of negative concentration".into() });
    });
    p.case("permutation-invariant", |p| {
        let goal = fixture_goal(false);
        let data = fixture_data();
        let permuted = vec![data[3].clone(), data[1].clone(), data[0].clone(), data[2].clone()];
        let Some(a) = fitted_params(p, &fit(&goal, &LinearModel, &data, None), "original") else { return; };
        let Some(b) = fitted_params(p, &fit(&goal, &LinearModel, &permuted, None), "permuted") else { return; };
        p.close("slope", a.get(&slope()).copied().unwrap_or(0.0), b.get(&slope()).copied().unwrap_or(0.0), 1e-9);
        p.close("intercept", a.get(&intercept()).copied().unwrap_or(0.0), b.get(&intercept()).copied().unwrap_or(0.0), 1e-9);
    });
    p.case("rescaling-scales", |p| {
        let goal = fixture_goal(false);
        let scale = 7.5;
        let scaled: Vec<FitRow> = fixture_data().iter().map(|row| FitRow { t: row.t, y: row.y * scale, weight: row.weight }).collect();
        let Some(base) = fitted_params(p, &fit(&goal, &LinearModel, &fixture_data(), None), "base") else { return; };
        let Some(rescaled) = fitted_params(p, &fit(&goal, &LinearModel, &scaled, None), "rescaled") else { return; };
        for (name, base_value) in &base {
            p.close(format!("scale-{name:?}"), rescaled.get(name).copied().unwrap_or(0.0), base_value * scale, 1e-6);
        }
    });
    p.case("weight-scaling-invariant", |p| {
        let goal = fixture_goal(false);
        let scaled: Vec<FitRow> = fixture_data().iter().map(|row| FitRow { t: row.t, y: row.y, weight: row.weight * 7.0 }).collect();
        let Some(base) = fitted_params(p, &fit(&goal, &LinearModel, &fixture_data(), None), "base") else { return; };
        let Some(weighted) = fitted_params(p, &fit(&goal, &LinearModel, &scaled, None), "weighted") else { return; };
        for (name, base_value) in &base {
            p.close(format!("invariant-{name:?}"), weighted.get(name).copied().unwrap_or(0.0), *base_value, 1e-9);
        }
    });
    p.case("ill-conditioned-resolves", |p| {
        use emath_lab_core::calibration::NumericRankOracle;
        let ts = [1.0_f64, 1.0002, 1.0004];
        let data: Vec<FitRow> = ts.iter().map(|t| FitRow { t: *t, y: 0.5 * t + 2.0, weight: 1.0 }).collect();
        let fitted = BTreeMap::from([(slope(), 0.5), (intercept(), 2.0)]);
        let goal = FitGoal::new(vec![slope(), intercept()], SymbolId("response".into()));
        let oracle = NumericRankOracle::default();
        let verdict = oracle.structural_identifiability(&goal, &LinearModel, &data, &fitted).expect("3 rows over 2 parameters must serve a verdict");
        p.eq("directions", verdict.directions.len(), 2);
        for (symbol, interval) in &verdict.directions {
            p.demand(format!("tight-{symbol:?}"), interval.tight, format!("full-rank design certifies {symbol:?} tight, got {interval:?}"));
            p.demand(format!("finite-{symbol:?}"), interval.lo.is_finite() && interval.hi.is_finite(), format!("tight intervals are finite; {symbol:?}: {interval:?}"));
        }
    });
    p.case("rank-deficient-relaxed", |p| {
        use emath_lab_core::calibration::NumericRankOracle;
        let data: Vec<FitRow> = [2.0_f64; 3].iter().map(|t| FitRow { t: *t, y: 3.0, weight: 1.0 }).collect();
        let fitted = BTreeMap::from([(slope(), 0.5), (intercept(), 2.0)]);
        let goal = FitGoal::new(vec![slope(), intercept()], SymbolId("response".into()));
        let oracle = NumericRankOracle::default();
        let verdict = oracle.structural_identifiability(&goal, &LinearModel, &data, &fitted).expect("rank-deficient design must serve relaxed verdict");
        p.eq("directions", verdict.directions.len(), 2);
        for (symbol, interval) in &verdict.directions {
            p.demand(format!("relaxed-{symbol:?}"), !interval.tight, format!("rank-deficient design reports {symbol:?} relaxed, got {interval:?}"));
            p.eq(format!("line-{symbol:?}"), (interval.lo, interval.hi), (f64::NEG_INFINITY, f64::INFINITY));
        }
    });
    p.case("non-finite-output-refused", |p| {
        let mut goal = FitGoal::new(vec![SymbolId("a".into())], SymbolId("y".into()));
        goal.initial.insert(SymbolId("a".into()), -1.0);
        let data = vec![FitRow { t: 0.0, y: 4.0, weight: 1.0 }];
        p.demand("model-error", matches!(fit(&goal, &NegativeDomainModel, &data, None), FitOutcome::ModelError { .. }), "non-finite SSE must refuse as ModelError");
    });
    p.case("non-finite-jacobian-refused", |p| {
        let mut goal = FitGoal::new(vec![SymbolId("a".into())], SymbolId("y".into()));
        goal.initial.insert(SymbolId("a".into()), 0.0);
        let data = vec![FitRow { t: 0.0, y: 1.0, weight: 1.0 }];
        p.demand("model-error", matches!(fit(&goal, &EdgeDomainModel, &data, None), FitOutcome::ModelError { .. }), "non-finite Jacobian must refuse as ModelError");
    });
    p.case("duplicate-observable-refused", |p| {
        let mut payload = pk_fixture_payload();
        payload.data.push(("conc_time".into(), vec!["9.0".into(); 4]));
        let error = FitGoal::from_payload(&payload, "conc_time").expect_err("second observable row must refuse");
        p.demand("duplicate", matches!(error, FitPayloadError::DuplicateObservableRow { .. }), format!("got {error:?}"));
    });
    p.case("provenance-binds-weights", |p| {
        use emath_lab_core::calibration::provenance;
        let plain = fixture_goal(false);
        let mut weighted = plain.clone();
        weighted.weights = ResidualWeights(BTreeMap::from([(slope(), 2.0)]));
        let data = fixture_data();
        let fitted = BTreeMap::from([(slope(), 2.0), (intercept(), 1.0)]);
        p.ne("hash-differs", provenance(&plain, &data, &fitted), provenance(&weighted, &data, &fitted));
    });
    p.finish();
}
