//! Planner witnesses: artifact-class preservation under provider and
//! budget growth, node-budget exhaustion (E-RES-100), and the capability
//! matrix admitting supported / refusing unsupported providers with
//! stable codes.

use emath_ir::{
    DeterminismPolicy, EvidenceLevel, ExactnessPolicy, FallbackPolicy, Goal, GoalId, GoalKind,
    GoalRequirements, TargetProfile,
};
use emath_plan::{plan, PlannerConfig, PlanningOutcome};
use emath_provider_api::{
    CapabilitySpec, CapabilityTable, ProviderIsolation, ProviderLock, ProviderRegistry,
    RegistryConfig, RepresentationSpec,
};

fn goal_with_produce(produce: &str) -> Goal {
    let mut goal = Goal {
        id: GoalId(1),
        kind: GoalKind::Evaluate,
        target: "y".into(),
        expression: None,
        requirements: GoalRequirements {
            evidence: EvidenceLevel::E1,
            exactness: ExactnessPolicy::Exact,
            determinism: DeterminismPolicy::Required,
            target: TargetProfile {
                family: "rust".into(),
                triple: None,
                features: vec![],
            },
            fallback: FallbackPolicy::Diagnostic,
            produce: String::new(),
        },
        source: emath_core::Span::default(),
    };
    goal.requirements.produce = produce.to_string();
    goal
}

fn provider_table(name: &str) -> CapabilityTable {
    CapabilityTable {
        capabilities: vec![CapabilitySpec {
            name: name.to_string(),
            semantic_subset: "rust".into(),
            representations: vec![RepresentationSpec {
                name: "f64".into(),
                exact_relation: "bit-identical".into(),
                encode_cost: 0,
            }],
            exactness: vec!["exact".into()],
            failure_modes: vec![],
            checker_bindings: vec!["sir-checker".into()],
        }],
        isolation: ProviderIsolation::Static,
        lock: ProviderLock::Unlocked,
        maximum_evidence: EvidenceLevel::E2,
        deterministic: true,
    }
}

/// Resolution monotonicity (total artifact protocol): adding a
/// provider or enlarging budgets must never destroy an artifact class
/// that was previously reachable. A goal that selected a plan with
/// one provider must still select the same class after another
/// provider registers and after the budgets grow.
#[test]
fn adding_providers_or_budget_preserves_the_artifact_class() {
    let goal = goal_with_produce("target");
    let mut registry = ProviderRegistry::new(RegistryConfig::static_only());
    registry
        .register(
            "p1",
            ProviderIsolation::Static,
            provider_table("evaluate.target"),
        )
        .expect("sample registration must succeed");
    let config = PlannerConfig::default();
    let baseline = match plan(&goal, &registry, &config) {
        PlanningOutcome::Selected { plan, .. } => plan.artifact_class,
        other => panic!("baseline goal must select a plan, got {other:?}"),
    };

    registry
        .register(
            "p2",
            ProviderIsolation::Static,
            provider_table("evaluate.target"),
        )
        .expect("second registration must succeed");
    let widened = match plan(&goal, &registry, &config) {
        PlanningOutcome::Selected { plan, .. } => plan.artifact_class,
        other => panic!("adding a provider must not destroy the plan, got {other:?}"),
    };
    assert_eq!(baseline, widened, "provider growth changed the class");

    let generous = PlannerConfig {
        max_nodes: config.max_nodes.saturating_mul(4),
        max_candidates: config.max_candidates.saturating_mul(4),
        ..config
    };
    let enlarged = match plan(&goal, &registry, &generous) {
        PlanningOutcome::Selected { plan, .. } => plan.artifact_class,
        other => panic!("budget growth must not destroy the plan, got {other:?}"),
    };
    assert_eq!(baseline, enlarged, "budget growth changed the class");
}

#[test]
fn node_budget_refuses_oversized_plan_dag() {
    let mut registry = ProviderRegistry::new(RegistryConfig::static_only());
    registry
        .register(
            "p1",
            ProviderIsolation::Static,
            provider_table("evaluate.target"),
        )
        .expect("sample registration must succeed");
    let outcome = plan(
        &goal_with_produce("target"),
        &registry,
        &PlannerConfig {
            max_nodes: 0,
            ..PlannerConfig::default()
        },
    );
    match &outcome {
        PlanningOutcome::Exhausted { inspection, .. } => assert!(
            inspection
                .budget
                .as_deref()
                .unwrap_or_default()
                .contains("E-RES-100"),
            "E-RES-100 must be issued in the exhausted inspection: {outcome:?}"
        ),
        other => panic!("max_nodes=0 must exhaust, got {other:?}"),
    }
}

/// Capability matrix: only a provider whose descriptor matches the
/// goal is selected; estimate-only and wrong-produce providers are
/// refused with stable codes; public IR carries `ProviderRef` ids,
/// not upstream descriptor types. An unsupported-only registry
/// falls back to the diagnostic disposition.
#[test]
fn capability_matrix_admits_supported_and_refuses_unsupported() {
    let goal = goal_with_produce("target");
    let mut registry = ProviderRegistry::new(RegistryConfig::static_only());
    registry
        .register(
            "exact-ok",
            ProviderIsolation::Static,
            provider_table("evaluate.target"),
        )
        .expect("exact provider must register");
    let mut estimate = provider_table("evaluate.target");
    estimate.capabilities[0].exactness = vec!["estimate".into()];
    registry
        .register("estimate-only", ProviderIsolation::Static, estimate)
        .expect("estimate provider must register");
    registry
        .register(
            "wrong-produce",
            ProviderIsolation::Static,
            provider_table("evaluate.other"),
        )
        .expect("wrong-produce provider must register");

    let outcome = plan(&goal, &registry, &PlannerConfig::default());
    let (selected, inspection) = match outcome {
        PlanningOutcome::Selected { plan, inspection } => (plan, inspection),
        other => panic!("supported provider must select a plan, got {other:?}"),
    };
    assert_eq!(inspection.candidates, vec!["exact-ok".to_string()]);
    assert!(
        inspection
            .exclusions
            .iter()
            .any(|(id, code, _)| id == "estimate-only" && code == "E-PROV-515"),
        "estimate-only must be refused for an exact goal: {:?}",
        inspection.exclusions
    );
    assert!(
        inspection
            .exclusions
            .iter()
            .any(|(id, code, _)| id == "wrong-produce" && code == "E-PROV-512"),
        "wrong produce must be refused: {:?}",
        inspection.exclusions
    );
    let explained = inspection.explain();
    assert!(explained.contains("exact-ok"));
    assert!(explained.contains("E-PROV-515"));
    assert!(explained.contains("E-PROV-512"));
    let provider_ids: Vec<&str> = selected
        .nodes
        .values()
        .filter_map(|node| node.provider.as_ref().map(|provider| provider.id.as_str()))
        .collect();
    assert!(
        provider_ids.iter().all(|id| *id == "exact-ok"),
        "public IR must name the admitted provider by id, got {provider_ids:?}"
    );

    let mut unsupported = ProviderRegistry::new(RegistryConfig::static_only());
    let mut estimate_only = provider_table("evaluate.target");
    estimate_only.capabilities[0].exactness = vec!["estimate".into()];
    unsupported
        .register("estimate-only", ProviderIsolation::Static, estimate_only)
        .expect("estimate-only provider must register");
    let fallback = plan(&goal, &unsupported, &PlannerConfig::default());
    match fallback {
        PlanningOutcome::NoEligible {
            disposition,
            reasons,
            ..
        } => {
            assert_eq!(disposition.name(), "diagnostic");
            assert!(
                reasons.iter().any(|reason| reason.contains("E-PROV-515")),
                "unsupported-only registry must refuse with exactness: {reasons:?}"
            );
        }
        other => panic!("unsupported-only registry must fall back, got {other:?}"),
    }
}
