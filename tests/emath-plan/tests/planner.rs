//! Planner: artifact-class preservation under provider/budget growth,
//! node-budget exhaustion (E-RES-100), capability matrix with stable
//! codes, deterministic combinations, plan identity bound to goal
//! semantics.

use emath_ir::{
    DeterminismPolicy, EvidenceLevel, ExactnessPolicy, FallbackPolicy, Goal, GoalId, GoalKind,
    GoalPayload, GoalRequirements, TargetProfile,
};
use emath_plan::{combination_name, plan, PlannerConfig, PlanningOutcome};
use emath_provider_api::{
    CapabilitySpec, CapabilityTable, ProviderIsolation, ProviderLock, ProviderRegistry,
    RegistryConfig, RepresentationSpec,
};
use emath_test_harness::Probe;

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
        payload: GoalPayload::default(),
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

fn register_ok(registry: &mut ProviderRegistry, id: &str, table: CapabilityTable) {
    registry
        .register(id, ProviderIsolation::Static, table)
        .expect("sample registration must succeed");
}

#[test]
fn planner_artifact_and_capability_matrix() {
    let mut p = Probe::new(
        "adding providers/budget never destroys artifact class; max_nodes=0 is E-RES-100; exact-ok selected, estimate E-PROV-515, wrong produce E-PROV-512; plan_id binds goal semantics",
    );
    p.case("class-preserved", |p| {
        let goal = goal_with_produce("target");
        let mut registry = ProviderRegistry::new(RegistryConfig::static_only());
        register_ok(&mut registry, "p1", provider_table("evaluate.target"));
        let config = PlannerConfig::default();
        let baseline = match plan(&goal, &registry, &config) {
            PlanningOutcome::Selected { plan, .. } => plan.artifact_class,
            other => {
                p.fail("baseline", format!("baseline goal must select a plan, got {other:?}"));
                return;
            }
        };
        register_ok(&mut registry, "p2", provider_table("evaluate.target"));
        match plan(&goal, &registry, &config) {
            PlanningOutcome::Selected { plan, .. } => {
                p.eq("provider-growth", plan.artifact_class.clone(), baseline.clone());
            }
            other => {
                p.fail("widened", format!("adding a provider must not destroy the plan, got {other:?}"));
            }
        }
        let generous = PlannerConfig {
            max_nodes: config.max_nodes.saturating_mul(4),
            max_candidates: config.max_candidates.saturating_mul(4),
            ..config
        };
        match plan(&goal, &registry, &generous) {
            PlanningOutcome::Selected { plan, .. } => {
                p.eq("budget-growth", plan.artifact_class.clone(), baseline.clone());
            }
            other => {
                p.fail("enlarged", format!("budget growth must not destroy the plan, got {other:?}"));
            }
        }
    });
    p.case("node-budget-e-res-100", |p| {
        let mut registry = ProviderRegistry::new(RegistryConfig::static_only());
        register_ok(&mut registry, "p1", provider_table("evaluate.target"));
        match plan(
            &goal_with_produce("target"),
            &registry,
            &PlannerConfig {
                max_nodes: 0,
                ..PlannerConfig::default()
            },
        ) {
            PlanningOutcome::Exhausted { inspection, .. } => {
                p.contains(
                    "code",
                    inspection.budget.as_deref().unwrap_or_default(),
                    "E-RES-100",
                );
            }
            other => {
                p.fail("exhausted", format!("max_nodes=0 must exhaust, got {other:?}"));
            }
        }
    });
    p.case("capability-matrix", |p| {
        let goal = goal_with_produce("target");
        let mut registry = ProviderRegistry::new(RegistryConfig::static_only());
        register_ok(&mut registry, "exact-ok", provider_table("evaluate.target"));
        let mut estimate = provider_table("evaluate.target");
        estimate.capabilities[0].exactness = vec!["estimate".into()];
        register_ok(&mut registry, "estimate-only", estimate);
        register_ok(&mut registry, "wrong-produce", provider_table("evaluate.other"));
        match plan(&goal, &registry, &PlannerConfig::default()) {
            PlanningOutcome::Selected { plan, inspection } => {
                p.eq("candidates", inspection.candidates.clone(), vec!["exact-ok".to_string()]);
                p.demand(
                    "e-prov-515",
                    inspection
                        .exclusions
                        .iter()
                        .any(|(id, code, _)| id == "estimate-only" && code == "E-PROV-515"),
                    format!("estimate-only must be E-PROV-515: {:?}", inspection.exclusions),
                );
                p.demand(
                    "e-prov-512",
                    inspection
                        .exclusions
                        .iter()
                        .any(|(id, code, _)| id == "wrong-produce" && code == "E-PROV-512"),
                    format!("wrong produce must be E-PROV-512: {:?}", inspection.exclusions),
                );
                let explained = inspection.explain();
                p.contains("explain-ok", &explained, "exact-ok");
                p.contains("explain-515", &explained, "E-PROV-515");
                p.contains("explain-512", &explained, "E-PROV-512");
                let provider_ids: Vec<&str> = plan
                    .nodes
                    .values()
                    .filter_map(|node| node.provider.as_ref().map(|provider| provider.id.as_str()))
                    .collect();
                p.demand(
                    "public-ids",
                    provider_ids.iter().all(|id| *id == "exact-ok"),
                    format!("public IR must name exact-ok, got {provider_ids:?}"),
                );
            }
            other => {
                p.fail("selected", format!("supported provider must select a plan, got {other:?}"));
            }
        }
        let mut unsupported = ProviderRegistry::new(RegistryConfig::static_only());
        let mut estimate_only = provider_table("evaluate.target");
        estimate_only.capabilities[0].exactness = vec!["estimate".into()];
        register_ok(&mut unsupported, "estimate-only", estimate_only);
        match plan(&goal, &unsupported, &PlannerConfig::default()) {
            PlanningOutcome::NoEligible {
                disposition,
                reasons,
                ..
            } => {
                p.eq("disposition", disposition.name(), "diagnostic");
                p.demand(
                    "e-prov-515",
                    reasons.iter().any(|reason| reason.contains("E-PROV-515")),
                    format!("unsupported-only registry must refuse with exactness: {reasons:?}"),
                );
            }
            other => {
                p.fail("fallback", format!("unsupported-only registry must fall back, got {other:?}"));
            }
        }
    });
    p.case("combination-name", |p| {
        let goal = goal_with_produce("target");
        let mut registry = ProviderRegistry::new(RegistryConfig::static_only());
        register_ok(&mut registry, "p1", provider_table("evaluate.target"));
        let config = PlannerConfig::default();
        match plan(&goal, &registry, &config) {
            PlanningOutcome::Selected { inspection, .. } => {
                p.eq(
                    "combination",
                    inspection.combination.clone(),
                    Some("evaluate:interpreter:p1".to_string()),
                );
                let second = plan(&goal, &registry, &config);
                p.eq(
                    "deterministic",
                    second.inspection().combination.clone(),
                    inspection.combination.clone(),
                );
                p.contains("explain", &inspection.explain(), "combination: evaluate:interpreter:p1");
                p.contains("json", &inspection.to_json(), "evaluate:interpreter:p1");
            }
            other => {
                p.fail("selected", format!("goal must select a plan, got {other:?}"));
            }
        }
        let mut base = goal_with_produce("target");
        base.kind = GoalKind::Solve;
        p.eq("solve", combination_name(&base, "p1"), "solve:newton-bracket:p1".to_string());
        base.kind = GoalKind::Differentiate;
        p.eq("diff", combination_name(&base, "p1"), "differentiate:dual-forward:p1".to_string());
        base.kind = GoalKind::Optimize;
        p.eq("opt", combination_name(&base, "p1"), "optimize:newton-hessian:p1".to_string());
        base.kind = GoalKind::Integrate;
        p.eq("int", combination_name(&base, "p1"), "integrate:quadrature:p1".to_string());
        base.kind = GoalKind::Evaluate;
        p.eq("eval", combination_name(&base, "p1"), "evaluate:interpreter:p1".to_string());
        base.kind = GoalKind::Custom(emath_core::SchemaId("fit".into()));
        base.payload = GoalPayload {
            method: "levenberg-marquardt".to_string(),
            ..GoalPayload::default()
        };
        p.eq(
            "custom",
            combination_name(&base, "p1"),
            "custom:levenberg-marquardt:p1".to_string(),
        );
    });
    p.case("plan-id-binds-semantics", |p| {
        let mut registry = ProviderRegistry::new(RegistryConfig::static_only());
        let mut multi = provider_table("evaluate.target");
        multi.capabilities.push({
            let mut spec = multi.capabilities[0].clone();
            spec.name = "differentiate.target".into();
            spec
        });
        multi.capabilities.push({
            let mut spec = multi.capabilities[0].clone();
            spec.name = "solve.target".into();
            spec
        });
        register_ok(&mut registry, "p1", multi);
        let config = PlannerConfig::default();
        let mut differentiate = goal_with_produce("target");
        differentiate.kind = GoalKind::Differentiate;
        differentiate.payload.wrt = vec!["x".into()];
        let mut solve = goal_with_produce("target");
        solve.kind = GoalKind::Solve;
        let plan_id = |goal: &Goal| match plan(goal, &registry, &config) {
            PlanningOutcome::Selected { plan, .. } => Some(plan.plan_id.0),
            _ => None,
        };
        match (plan_id(&differentiate), plan_id(&differentiate)) {
            (Some(first), Some(again)) => {
                p.eq("stable", first.clone(), again);
                match plan_id(&solve) {
                    Some(other) => {
                        p.ne("slot-collision", first.clone(), other);
                    }
                    None => {
                        p.fail("solve", "solve goal must select a plan");
                    }
                }
                let mut different_wrt = goal_with_produce("target");
                different_wrt.kind = GoalKind::Differentiate;
                different_wrt.payload.wrt = vec!["x".into(), "y".into()];
                match plan_id(&different_wrt) {
                    Some(wrt) => {
                        p.ne("wrt", first.clone(), wrt);
                    }
                    None => {
                        p.fail("wrt", "different wrt must still select");
                    }
                }
            }
            _ => {
                p.fail("diff", "differentiate goal must select a plan");
            }
        }
    });
    p.case("unselected-no-combination", |p| {
        let outcome = plan(
            &goal_with_produce("target"),
            &ProviderRegistry::new(RegistryConfig::static_only()),
            &PlannerConfig::default(),
        );
        p.demand(
            "no-eligible",
            matches!(outcome, PlanningOutcome::NoEligible { .. }),
            format!("an empty registry must refuse planning, got {outcome:?}"),
        );
        p.eq("combination", outcome.inspection().combination.clone(), None);
    });
    p.finish();
}
