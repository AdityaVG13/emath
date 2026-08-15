//!: MSL conformance ladder.
//!
//! Per-feature coverage across corpus tiers (syntax/import, name/type/
//! instantiation, flattened equations, structural analysis, simulation
//! reference results), never a single percentage. Reports are
//! deterministic and provider-free.

use emath_core::fnv1a64_bytes;

use crate::lower::DaePlan;
use crate::provider::SimulationResult;
use crate::structural::StructuralModel;

/// MSL conformance tier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    /// Syntax and import of the retained corpus.
    SyntaxImport,
    /// Name/type checking and instantiation.
    NameTypeInstantiation,
    /// Flattened equation diagnostics.
    FlattenedEquations,
    /// Structural analysis.
    StructuralAnalysis,
    /// Simulation reference results.
    SimulationReference,
}

impl Tier {
    /// Tier name (deterministic, lowercase).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::SyntaxImport => "syntax-import",
            Self::NameTypeInstantiation => "name-type-instantiation",
            Self::FlattenedEquations => "flattened-equations",
            Self::StructuralAnalysis => "structural-analysis",
            Self::SimulationReference => "simulation-reference",
        }
    }

    /// Ordinal tier index.
    #[must_use]
    pub const fn index(self) -> u8 {
        match self {
            Self::SyntaxImport => 1,
            Self::NameTypeInstantiation => 2,
            Self::FlattenedEquations => 3,
            Self::StructuralAnalysis => 4,
            Self::SimulationReference => 5,
        }
    }
}

/// Per-feature outcome.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeatureStatus {
    /// Feature covered by a passing check.
    Pass,
    /// Feature covered by a failing check.
    Fail,
    /// Feature not exercised by this fixture.
    Skipped,
}

/// One ladder row: feature, status, note.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeatureResult {
    /// Feature name.
    pub feature: &'static str,
    /// Status.
    pub status: FeatureStatus,
    /// Note.
    pub note: String,
}

/// Deterministic per-feature conformance report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConformanceReport {
    /// Highest tier evaluated.
    pub tier: Tier,
    /// Rows in tier order.
    pub results: Vec<FeatureResult>,
    identity: u64,
}

impl ConformanceReport {
    /// Deterministic canonical rendering.
    #[must_use]
    pub fn canonical(&self) -> String {
        format!(
            "msl:{}:{}",
            self.tier.name(),
            self.results
                .iter()
                .map(|row| format!("{}={:?}", row.feature, row.status))
                .collect::<Vec<_>>()
                .join(",")
        )
    }

    /// FNV-1a64 identity over the canonical rendering.
    #[must_use]
    pub fn content_identity(&self) -> u64 {
        self.identity
    }

    /// Feature status for a named feature.
    #[must_use]
    pub fn status_of(&self, feature: &str) -> Option<FeatureStatus> {
        self.results
            .iter()
            .find(|row| row.feature == feature)
            .map(|row| row.status)
    }
}

/// Evaluates the ladder for a fixture: tier 1-4 directly, tier 5 in
/// [`reference_result`].
#[must_use]
pub fn evaluate_msl(model: &StructuralModel, plan: Option<&DaePlan>) -> ConformanceReport {
    let issues = model.validate();
    let mut results = Vec::new();
    results.push(FeatureResult {
        feature: "construct-census",
        status: if model.components.iter().all(|component| {
            crate::map::classify(match component.kind {
                crate::structural::ComponentKind::Model
                | crate::structural::ComponentKind::Block => "equation",
                crate::structural::ComponentKind::Connector => "connector",
                crate::structural::ComponentKind::Record => "record",
            })
            .is_some()
        }) {
            FeatureStatus::Pass
        } else {
            FeatureStatus::Fail
        },
        note: "all component categories map to known constructs".into(),
    });
    results.push(FeatureResult {
        feature: "unique-names",
        status: if issues.iter().any(|issue| issue.code == "E-NAME-020") {
            FeatureStatus::Fail
        } else {
            FeatureStatus::Pass
        },
        note: "no duplicate variable names".into(),
    });
    results.push(FeatureResult {
        feature: "derivative-targets",
        status: if issues.iter().any(|issue| issue.code == "E-TYPE-101") {
            FeatureStatus::Fail
        } else {
            FeatureStatus::Pass
        },
        note: "derivatives target states only".into(),
    });
    results.push(FeatureResult {
        feature: "dimensional-consistency",
        status: if issues.iter().any(|issue| issue.code.starts_with("E-UNIT")) {
            FeatureStatus::Fail
        } else {
            FeatureStatus::Pass
        },
        note: "all equations dimensionally consistent".into(),
    });
    results.push(FeatureResult {
        feature: "causal-completion",
        status: match plan {
            Some(plan) if plan.tearing.is_empty() => FeatureStatus::Pass,
            Some(_) => FeatureStatus::Fail,
            None => FeatureStatus::Skipped,
        },
        note: "structural analysis completes without tearing".into(),
    });

    let tier = if plan.is_some() {
        Tier::StructuralAnalysis
    } else {
        Tier::FlattenedEquations
    };
    let mut report = ConformanceReport {
        tier,
        results,
        identity: 0,
    };
    report.identity = fnv1a64_bytes(report.canonical().as_bytes());
    report
}

/// Tier-5 row: compare a simulation against reference results.
#[must_use]
pub fn reference_result(
    simulation: &SimulationResult,
    expected_position: f64,
    expected_velocity: f64,
    tolerance: f64,
) -> FeatureResult {
    let position_ok = (simulation.final_position - expected_position).abs() <= tolerance;
    let velocity_ok = (simulation.final_velocity - expected_velocity).abs() <= tolerance;
    FeatureResult {
        feature: "simulation-reference",
        status: if position_ok && velocity_ok {
            FeatureStatus::Pass
        } else {
            FeatureStatus::Fail
        },
        note: format!(
            "final ({:e}, {:e}) vs reference ({expected_position:e}, {expected_velocity:e}) tol {tolerance:e}",
            simulation.final_position, simulation.final_velocity
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::SimulationConfig;
    use crate::structural::{Component, ComponentKind};
    use crate::structural::{
        Dimensions, EqExpr, Equation, InitialCondition, Unit, VariableDecl, VariableKind,
    };
    use emath_ir::TypeNode;

    fn fixture() -> StructuralModel {
        StructuralModel {
            components: vec![Component {
                name: "mass".into(),
                kind: ComponentKind::Model,
            }],
            variables: vec![
                VariableDecl {
                    name: "m".into(),
                    kind: VariableKind::Parameter,
                    unit: Unit::new("kg".into(), Dimensions::kilograms()),
                    ty: TypeNode::Float64,
                },
                VariableDecl {
                    name: "k".into(),
                    kind: VariableKind::Parameter,
                    unit: Unit::new(
                        "kg/s2".into(),
                        Dimensions::kilograms()
                            .div(Dimensions::seconds())
                            .div(Dimensions::seconds()),
                    ),
                    ty: TypeNode::Float64,
                },
                VariableDecl {
                    name: "x".into(),
                    kind: VariableKind::State,
                    unit: Unit::meters(),
                    ty: TypeNode::Float64,
                },
                VariableDecl {
                    name: "v".into(),
                    kind: VariableKind::State,
                    unit: Unit::new(
                        "m/s".into(),
                        Dimensions::meters().div(Dimensions::seconds()),
                    ),
                    ty: TypeNode::Float64,
                },
            ],
            equations: vec![
                Equation {
                    lhs: EqExpr::Der("x".into()),
                    rhs: EqExpr::Var("v".into()),
                    origin: "mass".into(),
                },
                Equation {
                    lhs: EqExpr::Der("v".into()),
                    rhs: EqExpr::Div(
                        Box::new(EqExpr::Neg(Box::new(EqExpr::Mul(
                            Box::new(EqExpr::Var("k".into())),
                            Box::new(EqExpr::Var("x".into())),
                        )))),
                        Box::new(EqExpr::Var("m".into())),
                    ),
                    origin: "mass".into(),
                },
            ],
            initial_conditions: vec![InitialCondition {
                target: "x".into(),
                value: EqExpr::constant(1.0),
            }],
            connections: vec![],
            events: vec![],
        }
    }

    #[test]
    fn ladder_reports_per_feature_status() {
        let model = fixture();
        let plan = crate::lower::lower(&model).unwrap();
        let report = evaluate_msl(&model, Some(&plan));
        assert_eq!(report.tier, Tier::StructuralAnalysis);
        assert_eq!(
            report.status_of("causal-completion"),
            Some(FeatureStatus::Pass)
        );
        assert_eq!(
            report.status_of("dimensional-consistency"),
            Some(FeatureStatus::Pass)
        );
        let again = evaluate_msl(&model, Some(&plan));
        assert_eq!(report.canonical(), again.canonical());
        assert_eq!(report.content_identity(), again.content_identity());
    }

    #[test]
    fn unit_mismatch_fails_ladder_tier() {
        let mut model = fixture();
        // Position expressed in seconds: dimensionally wrong.
        model.equations[0].rhs = EqExpr::Add(
            Box::new(EqExpr::Var("v".into())),
            Box::new(EqExpr::constant(1.0)),
        );
        let report = evaluate_msl(&model, None);
        assert_eq!(
            report.status_of("dimensional-consistency"),
            Some(FeatureStatus::Fail)
        );
    }

    #[test]
    fn reference_mismatch_is_fail() {
        let model = fixture();
        let plan = crate::lower::lower(&model).unwrap();
        let outcome = crate::provider::simulate(
            &model,
            &plan,
            &std::collections::BTreeMap::from([("m".to_string(), 1.0), ("k".to_string(), 1.0)]),
            &SimulationConfig::default(),
            &emath_runtime::Budget::default(),
        );
        let emath_runtime::Outcome::Resolved { value, .. } = outcome else {
            panic!("expected resolved simulation");
        };
        let row = reference_result(&value, 99.0, 99.0, 1e-12);
        assert_eq!(row.status, FeatureStatus::Fail);
        assert!(row.note.contains("reference"));
    }
}
