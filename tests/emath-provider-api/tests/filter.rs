//! Compatibility filter tests.

use emath_core::Span;
use emath_ir::{
    DeterminismPolicy, EvidenceLevel, ExactnessPolicy, FallbackPolicy, Goal, GoalId, GoalKind,
    GoalRequirements, TargetProfile,
};
use emath_provider_api::descriptor::{
    CapabilitySpec, CapabilityTable, ProviderIsolation, RepresentationSpec,
};
use emath_provider_api::filter::{Compatibility, filter_goal};
use emath_provider_api::registry::{ProviderRegistry, RegistryConfig};
use emath_test_harness::Probe;

fn registry_with_exactness(tokens: Vec<String>) -> ProviderRegistry {
    let mut registry = ProviderRegistry::new(RegistryConfig::static_only());
    registry.register("provider-a", ProviderIsolation::Static, CapabilityTable { capabilities: vec![CapabilitySpec { name: "evaluate".into(), semantic_subset: "host".into(), representations: vec![RepresentationSpec { name: "native".into(), exact_relation: "identity".into(), encode_cost: 0 }], exactness: tokens, failure_modes: vec![], checker_bindings: vec![] }], ..CapabilityTable::default() }).expect("static registration admitted");
    registry
}

fn goal_with_exactness(exactness: ExactnessPolicy) -> Goal {
    Goal { id: GoalId(0), kind: GoalKind::Evaluate, target: "x".into(), expression: None, requirements: GoalRequirements { evidence: EvidenceLevel::E0, exactness, determinism: DeterminismPolicy::Unspecified, target: TargetProfile { family: "host".into(), triple: None, features: vec![] }, produce: "rust.library".into(), fallback: FallbackPolicy::NativeOnly }, payload: emath_ir::GoalPayload::default(), source: Span::default() }
}

fn verdict_for(exactness: ExactnessPolicy, tokens: Vec<String>) -> Compatibility {
    let verdicts = filter_goal(&goal_with_exactness(exactness), &registry_with_exactness(tokens));
    verdicts.iter().find(|v| v.provider == "provider-a").expect("candidate present").compatibility.clone()
}

#[test]
fn exactness_filter() {
    let mut p = Probe::new("estimate-only providers serve estimate goals, nothing stricter");
    p.case("estimate-served", |p| {
        p.demand("compatible", verdict_for(ExactnessPolicy::Estimate, vec!["estimate".into()]).is_compatible(), "estimate goal served");
    });
    p.case("bounded-excluded", |p| {
        let excluded = verdict_for(ExactnessPolicy::Bounded { tolerance_literal: "1e-3".into() }, vec!["estimate".into()]);
        match excluded {
            Compatibility::Excluded { reasons } => p.demand("code", reasons.iter().any(|r| r.code == "E-PROV-515"), "bounded goal excludes estimate-only"),
            Compatibility::Compatible => p.fail("bounded", "estimate-only provider must be excluded"),
        };
    });
    p.case("undeclared-excluded", |p| {
        match verdict_for(ExactnessPolicy::AnyExplicit, vec![]) {
            Compatibility::Excluded { reasons } => p.demand("code", reasons.iter().any(|r| r.code == "E-PROV-515"), "undeclared exactness excluded"),
            Compatibility::Compatible => p.fail("any-explicit", "undeclared exactness must be excluded"),
        };
    });
    p.finish();
}
