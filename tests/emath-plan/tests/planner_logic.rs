//! Planner/filter: budget exhaustion by compatible count, produce-exact
//! capability matching, requirement polarity, lossy-path BFS.

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
use emath_test_harness::Probe;

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

fn goal_with_produce(produce: &str) -> Goal {
    let mut goal = Goal {
        id: GoalId(1),
        kind: GoalKind::Evaluate,
        target: "y".into(),
        expression: None,
        requirements: requirements(ExactnessPolicy::Exact),
        payload: emath_ir::GoalPayload::default(),
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

fn register(registry: &mut ProviderRegistry, id: &str, name: &str) {
    registry
        .register(id, ProviderIsolation::Static, provider_table(name))
        .expect("sample registration must succeed");
}

#[test]
fn planner_filter_laws() {
    let mut p = Probe::new(
        "one compatible plan is not exhausted; 9 is; exclusions name E-PROV-512; exact produce matching; Exact refuses all-lossy paths with E-PROV-515",
    );
    p.case("one-compatible-selects", |p| {
        let mut registry = ProviderRegistry::new(RegistryConfig::static_only());
        register(&mut registry, "p1", "evaluate.target");
        for index in 2..=9 {
            register(&mut registry, &format!("p{index}"), "evaluate.other");
        }
        match plan(&goal_with_produce("target"), &registry, &PlannerConfig::default()) {
            PlanningOutcome::Selected { plan, .. } => {
                p.eq("excluded", plan.excluded_candidates.len(), 8);
                p.demand(
                    "e-prov-512",
                    plan.excluded_candidates
                        .iter()
                        .all(|excluded| excluded.reason.contains("E-PROV-512")),
                    "every exclusion must name E-PROV-512",
                );
            }
            other => {
                p.fail("selected", format!("1 compatible + 8 excluded must select, got {other:?}"));
            }
        }
    });
    p.case("nine-compatible-exhausts", |p| {
        let mut registry = ProviderRegistry::new(RegistryConfig::static_only());
        for index in 0..9 {
            register(&mut registry, &format!("p{index}"), "evaluate.target");
        }
        p.demand(
            "exhausted",
            matches!(
                plan(&goal_with_produce("target"), &registry, &PlannerConfig::default()),
                PlanningOutcome::Exhausted { .. }
            ),
            "9 compatible must exhaust the 8-candidate horizon",
        );
    });
    p.case("excluded-trace", |p| {
        let mut registry = ProviderRegistry::new(RegistryConfig::static_only());
        register(&mut registry, "p1", "evaluate.target");
        register(&mut registry, "p2", "evaluate.plot");
        let trace = excluded_trace(&goal_with_produce("target"), &registry);
        p.eq("len", trace.len(), 1);
        p.eq("provider", trace[0].provider.as_str(), "p2");
        p.contains("code", &trace[0].reason, "E-PROV-512");
    });
    p.case("serves-kind-exact-produce", |p| {
        let goal = goal_with_produce("rust.library");
        let plot = CapabilitySpec {
            name: "evaluate.plot".into(),
            semantic_subset: "rust".into(),
            representations: vec![],
            exactness: vec!["exact".into()],
            failure_modes: vec![],
            checker_bindings: vec![],
        };
        p.demand("plot-no", !plot.serves_kind(&goal), "evaluate.plot must not serve rust.library");
        p.demand(
            "library-yes",
            CapabilitySpec {
                name: "evaluate.rust.library".into(),
                ..plot.clone()
            }
            .serves_kind(&goal),
            "evaluate.rust.library must serve",
        );
        p.demand(
            "bare-yes",
            CapabilitySpec {
                name: "evaluate".into(),
                ..plot
            }
            .serves_kind(&goal),
            "bare evaluate must serve",
        );
    });
    p.case("exactness-polarity", |p| {
        let exact = requirements(ExactnessPolicy::Exact);
        let estimate = requirements(ExactnessPolicy::Estimate);
        p.demand(
            "looser-child",
            requirements_preserved(&exact, &estimate),
            "Estimate child of Exact is allowed",
        );
        p.demand(
            "stricter-child",
            !requirements_preserved(&estimate, &exact),
            "Exact child of Estimate is refused",
        );
    });
    p.case("exact-skips-lossy", |p| {
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
        match find_conversion_path("a", "target", &conversions, &ExactnessPolicy::Exact) {
            Ok(path) => {
                p.eq("len", path.len(), 2);
                p.eq("via", path[0].conversion.to.as_str(), "safe");
                p.eq("from", path[1].conversion.from.as_str(), "safe");
            }
            Err(error) => {
                p.fail("path", format!("exact goal must find the conserving path: {error:?}"));
            }
        }
    });
    p.case("exact-all-lossy-refuses", |p| {
        let conversions = vec![Conversion {
            from: "a".into(),
            to: "target".into(),
            cost: 1,
            exact_relation: "irreversible",
        }];
        match find_conversion_path("a", "target", &conversions, &ExactnessPolicy::Exact) {
            Ok(path) => {
                p.fail("refuse", format!("all-lossy path must refuse, got {path:?}"));
            }
            Err(error) => {
                p.eq("code", error.code, "E-PROV-515");
            }
        }
    });
    p.case("estimate-accepts-lossy", |p| {
        let conversions = vec![Conversion {
            from: "a".into(),
            to: "target".into(),
            cost: 1,
            exact_relation: "irreversible",
        }];
        match find_conversion_path("a", "target", &conversions, &ExactnessPolicy::Estimate) {
            Ok(path) => {
                p.eq("len", path.len(), 1);
            }
            Err(error) => {
                p.fail("path", format!("estimate goal accepts a lossy path: {error:?}"));
            }
        }
    });
    p.finish();
}
