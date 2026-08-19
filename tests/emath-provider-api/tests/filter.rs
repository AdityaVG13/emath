//! Compatibility filter tests (origin `crates/emath-provider-api/src/filter.rs`).

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

fn registry_with_exactness(tokens: Vec<String>) -> ProviderRegistry {
    let mut registry = ProviderRegistry::new(RegistryConfig::static_only());
    registry
        .register(
            "provider-a",
            ProviderIsolation::Static,
            CapabilityTable {
                capabilities: vec![CapabilitySpec {
                    name: "evaluate".into(),
                    semantic_subset: "host".into(),
                    representations: vec![RepresentationSpec {
                        name: "native".into(),
                        exact_relation: "identity".into(),
                        encode_cost: 0,
                    }],
                    exactness: tokens,
                    failure_modes: vec![],
                    checker_bindings: vec![],
                }],
                ..CapabilityTable::default()
            },
        )
        .expect("static registration admitted");
    registry
}

fn goal_with_exactness(exactness: ExactnessPolicy) -> Goal {
    Goal {
        id: GoalId(0),
        kind: GoalKind::Evaluate,
        target: "x".into(),
        expression: None,
        requirements: GoalRequirements {
            evidence: EvidenceLevel::E0,
            exactness,
            determinism: DeterminismPolicy::Unspecified,
            target: TargetProfile {
                family: "host".into(),
                triple: None,
                features: vec![],
            },
            produce: "rust.library".into(),
            fallback: FallbackPolicy::NativeOnly,
        },
        source: Span::default(),
    }
}

fn verdict_for(exactness: ExactnessPolicy, tokens: Vec<String>) -> Compatibility {
    let verdicts = filter_goal(
        &goal_with_exactness(exactness),
        &registry_with_exactness(tokens),
    );
    let verdict = verdicts
        .iter()
        .find(|verdict| verdict.provider == "provider-a")
        .expect("candidate present");
    verdict.compatibility.clone()
}

#[test]
fn estimate_only_provider_serves_estimate_goal() {
    assert!(verdict_for(ExactnessPolicy::Estimate, vec!["estimate".into()]).is_compatible());
}

#[test]
fn estimate_only_provider_excluded_for_bounded_goal() {
    let excluded = verdict_for(
        ExactnessPolicy::Bounded {
            tolerance_literal: "1e-3".into(),
        },
        vec!["estimate".into()],
    );
    let reasons = match excluded {
        Compatibility::Excluded { reasons } => reasons,
        Compatibility::Compatible => panic!("estimate-only provider must be excluded"),
    };
    assert!(reasons.iter().any(|reason| reason.code == "E-PROV-515"));
}

#[test]
fn undeclared_exactness_provider_excluded_for_any_explicit_goal() {
    let excluded = verdict_for(ExactnessPolicy::AnyExplicit, vec![]);
    let reasons = match excluded {
        Compatibility::Excluded { reasons } => reasons,
        Compatibility::Compatible => panic!("undeclared exactness must be excluded"),
    };
    assert!(reasons.iter().any(|reason| reason.code == "E-PROV-515"));
}
