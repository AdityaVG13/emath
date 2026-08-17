//! Planner/filter witnesses: budget exhaustion by compatible
//! count, produce-exact capability matching, requirement polarity and
//! lossy-path BFS continuation.

use emath_ir::{
    DeterminismPolicy, EvidenceLevel, ExactnessPolicy, FallbackPolicy, Goal, GoalId, GoalKind,
    GoalRequirements, TargetProfile,
};
use emath_plan::planner::excluded_trace;
use emath_plan::{
    find_conversion_path, plan, requirements_preserved, Conversion, PlannerConfig, PlanningOutcome,
};
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
        requirements: requirements(ExactnessPolicy::Exact),
        source: emath_core::Span::default(),
    };
    goal.requirements.produce = produce.to_string();
    goal
}

fn requirements(exactness: ExactnessPolicy) -> GoalRequirements {
    GoalRequirements {
        evidence: EvidenceLevel::E1,
        exactness,
        determinism: DeterminismPolicy::Required,
        target: TargetProfile {
            family: "rust".into(),
            triple: None,
            features: vec![],
        },
        fallback: FallbackPolicy::Diagnostic,
        produce: String::new(),
    }
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

fn register(registry: &mut ProviderRegistry, id: &str, name: &str) {
    registry
        .register(id, ProviderIsolation::Static, provider_table(name))
        .expect("sample registration must succeed");
}

#[test]
fn one_compatible_plan_in_large_registry_is_not_exhausted() {
    let mut registry = ProviderRegistry::new(RegistryConfig::static_only());
    register(&mut registry, "p1", "evaluate.target");
    for index in 2..=9 {
        register(&mut registry, &format!("p{index}"), "evaluate.other");
    }
    let outcome = plan(
        &goal_with_produce("target"),
        &registry,
        &PlannerConfig::default(),
    );
    assert!(
        matches!(outcome, PlanningOutcome::Selected { .. }),
        "1 compatible + 8 excluded must select, got {outcome:?}"
    );
    if let PlanningOutcome::Selected { plan, .. } = &outcome {
        assert_eq!(plan.excluded_candidates.len(), 8);
        assert!(plan
            .excluded_candidates
            .iter()
            .all(|excluded| excluded.reason.contains("E-PROV-512")));
    }
}

#[test]
fn more_than_max_compatible_candidates_is_exhausted() {
    let mut registry = ProviderRegistry::new(RegistryConfig::static_only());
    for index in 0..9 {
        register(&mut registry, &format!("p{index}"), "evaluate.target");
    }
    let outcome = plan(
        &goal_with_produce("target"),
        &registry,
        &PlannerConfig::default(),
    );
    assert!(
        matches!(outcome, PlanningOutcome::Exhausted { .. }),
        "9 compatible must exhaust the 8-candidate horizon, got {outcome:?}"
    );
}

#[test]
fn excluded_trace_reports_real_exclusions() {
    let mut registry = ProviderRegistry::new(RegistryConfig::static_only());
    register(&mut registry, "p1", "evaluate.target");
    register(&mut registry, "p2", "evaluate.plot");
    let goal = goal_with_produce("target");
    let trace = excluded_trace(&goal, &registry);
    assert_eq!(trace.len(), 1);
    assert_eq!(trace[0].provider, "p2");
    assert!(trace[0].reason.contains("E-PROV-512"));
}

#[test]
fn serves_kind_requires_exact_produce() {
    let goal = goal_with_produce("rust.library");
    let plot = CapabilitySpec {
        name: "evaluate.plot".into(),
        semantic_subset: "rust".into(),
        representations: vec![],
        exactness: vec!["exact".into()],
        failure_modes: vec![],
        checker_bindings: vec![],
    };
    assert!(
        !plot.serves_kind(&goal),
        "evaluate.plot must not serve produce rust.library"
    );
    let library = CapabilitySpec {
        name: "evaluate.rust.library".into(),
        ..plot.clone()
    };
    assert!(library.serves_kind(&goal));
    let bare = CapabilitySpec {
        name: "evaluate".into(),
        ..plot
    };
    assert!(bare.serves_kind(&goal));
}

#[test]
fn exactness_preservation_polarity_matches_comment() {
    let exact = requirements(ExactnessPolicy::Exact);
    let estimate = requirements(ExactnessPolicy::Estimate);
    assert!(
        requirements_preserved(&exact, &estimate),
        "Estimate child of Exact is allowed (child looser)"
    );
    assert!(
        !requirements_preserved(&estimate, &exact),
        "Exact child of Estimate is refused (child stricter)"
    );
}

#[test]
fn exact_goal_keeps_searching_past_lossy_hit() {
    let conversions = vec![
        Conversion {
            from: "a".into(),
            to: "target".into(),
            cost: 1,
            exact_relation: "irreversible",
        },
        Conversion {
            from: "a".into(),
            to: "safe".into(),
            cost: 5,
            exact_relation: "value-conserving",
        },
        Conversion {
            from: "safe".into(),
            to: "target".into(),
            cost: 5,
            exact_relation: "value-conserving",
        },
    ];
    let path = find_conversion_path("a", "target", &conversions, &ExactnessPolicy::Exact)
        .expect("exact goal must find the conserving path");
    assert_eq!(path.len(), 2);
    assert_eq!(path[0].conversion.to, "safe");
    assert_eq!(path[1].conversion.from, "safe");
}

#[test]
fn exact_goal_refuses_when_every_path_is_lossy() {
    let conversions = vec![Conversion {
        from: "a".into(),
        to: "target".into(),
        cost: 1,
        exact_relation: "irreversible",
    }];
    let error = find_conversion_path("a", "target", &conversions, &ExactnessPolicy::Exact)
        .expect_err("all-lossy path must refuse for exact goal");
    assert_eq!(error.code, "E-PROV-515");
}

#[test]
fn estimate_goal_accepts_first_lossy_path() {
    let conversions = vec![Conversion {
        from: "a".into(),
        to: "target".into(),
        cost: 1,
        exact_relation: "irreversible",
    }];
    let path = find_conversion_path("a", "target", &conversions, &ExactnessPolicy::Estimate)
        .expect("estimate goal accepts a lossy path");
    assert_eq!(path.len(), 1);
}
