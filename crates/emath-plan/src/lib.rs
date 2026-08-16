//! Deterministic native resolution planner (Phase 1 bootstrap).
//!
//! Builds the resolution-plan DAG for `evaluate → rust.library` using the
//! native path: lower → execute(native) → check → package → admit. No
//! external providers are installed in Phase 1; every other candidate is
//! recorded as excluded with its reason (an honest trace, not a silent
//! fallback). Phase 6 adds the deterministic planner over the provider
//! registry: decomposition rules, representation planning, fallback
//! graphs, provider lifting, total dispositions, inspection and the plan
//! identity/cache.

#![forbid(unsafe_code)]

pub mod decompose;
pub mod dispositions;
pub mod fallback;
pub mod identity;
pub mod inspect;
pub mod lifting;
pub mod planner;
pub mod registry_helpers;
pub mod representations;

pub use decompose::{
    decompose, requirements_preserved, DecompositionRule, SubgoalDag, SubgoalNode,
};
pub use dispositions::{
    disposition_exhausted, disposition_for_plan, disposition_without_plan, ArtifactDisposition,
};
pub use fallback::{FallbackGraph, FallbackNode};
pub use identity::{plan_identity, provider_set_fingerprint, PlanCache, ProviderFingerprint};
pub use inspect::PlanInspection;
pub use lifting::{emit_provider_trait, lift_missing, LiftedMethod, ProviderTraitSpec};
pub use planner::{plan, PlannerConfig, PlanningOutcome, TieBreak};
pub use representations::{find_conversion_path, Conversion, ConversionNode, RepresentationError};

use emath_core::{ContentId, SchemaId};
use emath_ir::{
    EvidenceClaimId, ExcludedCandidate, GoalId, PlanNodeDef, PlanNodeId, PlanOperation,
    ProviderRef, ResolutionPlan,
};
use std::collections::BTreeMap;

pub const POLICY: &str = "native-deterministic.v1";
pub const PLAN_SCHEMA: &str = "emath.resolution-plan.v1";

/// Provider identities known to the constellation but not installed in
/// Phase 1; they are excluded with reasons in every plan.
pub const EXCLUDED_PROVIDERS: &[(&str, &str)] = &[
    ("phase2.expression", "adapter not installed until Phase 2"),
    ("phase3.structural", "adapter not installed until Phase 3"),
    (
        "phase4.symbolic",
        "optional symbolic provider, not installed",
    ),
    ("phase7.adapter", "adapter not installed until Phase 7"),
];

/// Build the deterministic native plan for a goal.
#[must_use]
pub fn native_plan(goal: GoalId, artifact_class: &str) -> ResolutionPlan {
    let mut nodes = BTreeMap::new();
    let lower = PlanNodeId(0);
    let execute = PlanNodeId(1);
    let check = PlanNodeId(2);
    let package = PlanNodeId(3);
    let admit = PlanNodeId(4);
    nodes.insert(
        lower,
        PlanNodeDef {
            id: lower,
            operation: PlanOperation::Lower,
            dependencies: vec![],
            provider: Some(ProviderRef {
                id: "emath.native".into(),
                version: "0.1.0".into(),
                implementation: ContentId("fnv1a64:0000000000000000".into()),
            }),
            checks: vec![],
            fallback: None,
            budget: Some("compile:1".into()),
        },
    );
    nodes.insert(
        execute,
        PlanNodeDef {
            id: execute,
            operation: PlanOperation::Execute,
            dependencies: vec![lower],
            provider: Some(ProviderRef {
                id: "emath.native".into(),
                version: "0.1.0".into(),
                implementation: ContentId("fnv1a64:0000000000000000".into()),
            }),
            checks: vec![],
            fallback: None,
            budget: Some("compile:1".into()),
        },
    );
    nodes.insert(
        check,
        PlanNodeDef {
            id: check,
            operation: PlanOperation::Check,
            dependencies: vec![execute],
            provider: Some(ProviderRef {
                id: "emath.native".into(),
                version: "0.1.0".into(),
                implementation: ContentId("fnv1a64:0000000000000000".into()),
            }),
            checks: vec![EvidenceClaimId(0)],
            fallback: None,
            budget: None,
        },
    );
    nodes.insert(
        package,
        PlanNodeDef {
            id: package,
            operation: PlanOperation::Package,
            dependencies: vec![check],
            provider: None,
            checks: vec![],
            fallback: None,
            budget: None,
        },
    );
    nodes.insert(
        admit,
        PlanNodeDef {
            id: admit,
            operation: PlanOperation::Admit,
            dependencies: vec![package],
            provider: None,
            checks: vec![EvidenceClaimId(0)],
            fallback: None,
            budget: None,
        },
    );
    let plan_bytes = canonical_plan(&nodes, admit, artifact_class, &goal.0.to_string());
    ResolutionPlan {
        schema: SchemaId(PLAN_SCHEMA.into()),
        plan_id: ContentId(plan_bytes),
        goal,
        policy: POLICY.to_string(),
        artifact_class: artifact_class.to_string(),
        nodes,
        root: admit,
        excluded_candidates: EXCLUDED_PROVIDERS
            .iter()
            .map(|(provider, reason)| ExcludedCandidate {
                provider: (*provider).to_string(),
                reason: (*reason).to_string(),
            })
            .collect(),
    }
}

fn canonical_plan(
    nodes: &BTreeMap<PlanNodeId, PlanNodeDef>,
    root: PlanNodeId,
    artifact_class: &str,
    goal_id: &str,
) -> String {
    let mut out = String::new();
    out.push_str("emath.plan.v1\n");
    out.push_str("goal ");
    out.push_str(goal_id);
    out.push('\n');
    out.push_str(artifact_class);
    out.push('\n');
    let root_line = format!("root {}\n", root.0);
    out.push_str(&root_line);
    for (id, node) in nodes {
        let head = format!(
            "{} {} {}",
            id.0,
            node.operation.name(),
            node.dependencies.len()
        );
        out.push_str(&head);
        let tail = node
            .dependencies
            .iter()
            .map(|dep| format!(" {}", dep.0))
            .collect::<Vec<_>>()
            .concat();
        out.push_str(&tail);
        out.push('\n');
    }
    out
}
