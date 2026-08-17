//! /06-011: deterministic planner and plan inspection.
//!
//! Phase 1 planner: ordered rules, bounded candidate retention, explicit
//! pruning and deterministic tie-breaks. Every exclusion is retained with
//! its reason; an exhausted budget yields a continuation/diagnostic per
//! the goal's fallback policy (`E-RES-100`, `E-GOAL-201`).

use crate::dispositions::{
    ArtifactDisposition, disposition_exhausted, disposition_for_plan, disposition_without_plan,
};
use crate::identity::plan_identity;
use crate::inspect::PlanInspection;
use emath_core::{ContentId, SchemaId};
use emath_ir::{
    EvidenceClaimId, ExcludedCandidate, Goal, GoalId, PlanNodeDef, PlanNodeId, PlanOperation,
    ProviderRef, ResolutionPlan,
};
use emath_provider_api::{Compatibility, ProviderRegistry, filter_goal};
use std::collections::BTreeMap;

/// Planner configuration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlannerConfig {
    /// Maximum candidate plans retained.
    pub max_candidates: usize,
    /// Maximum plan nodes.
    pub max_nodes: usize,
    /// Tie-break rule.
    pub tie_break: TieBreak,
    /// Planner policy name (binds to plan identity).
    pub policy: String,
}

impl Default for PlannerConfig {
    fn default() -> Self {
        Self {
            max_candidates: 8,
            max_nodes: 16,
            tie_break: TieBreak::CostAscendingId,
            policy: "deterministic-planner.v1".to_string(),
        }
    }
}

/// Deterministic tie-break rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TieBreak {
    /// Lowest conversion cost, then provider id.
    CostAscendingId,
    /// Provider id lexicographic.
    IdLexicographic,
}

/// Planning outcome, total over inputs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlanningOutcome {
    /// A plan was selected.
    Selected {
        plan: ResolutionPlan,
        inspection: PlanInspection,
    },
    /// No eligible plan; reasons and disposition per policy.
    NoEligible {
        reasons: Vec<String>,
        disposition: ArtifactDisposition,
        inspection: PlanInspection,
    },
    /// Planning budget exhausted; continuation or diagnostic per policy.
    Exhausted {
        continuation: String,
        disposition: ArtifactDisposition,
        inspection: PlanInspection,
    },
}

impl PlanningOutcome {
    /// Disposition name for reporting.
    #[must_use]
    pub fn disposition_name(&self) -> String {
        match self {
            Self::Selected { plan, .. } => plan.artifact_class.clone(),
            Self::NoEligible { disposition, .. } | Self::Exhausted { disposition, .. } => {
                disposition.name().to_string()
            }
        }
    }
}

/// Runs the deterministic planner over the goal and registry.
#[must_use]
pub fn plan(goal: &Goal, registry: &ProviderRegistry, config: &PlannerConfig) -> PlanningOutcome {
    let verdicts = filter_goal(goal, registry);
    let mut candidates: Vec<(u8, String)> = Vec::new();
    let mut exclusions: Vec<(String, String, String)> = Vec::new();
    for verdict in &verdicts {
        match &verdict.compatibility {
            Compatibility::Compatible => {
                let cost = estimate_cost(registry, &verdict.provider);
                candidates.push((cost, verdict.provider.clone()));
            }
            Compatibility::Excluded { reasons } => {
                let primary = reasons.first().expect("exclusion always has reasons");
                exclusions.push((
                    verdict.provider.clone(),
                    primary.code.to_string(),
                    primary.detail.clone(),
                ));
            }
        }
    }
    // Deterministic candidate ordering and tie-break.
    match config.tie_break {
        TieBreak::CostAscendingId => candidates
            .sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1))),
        TieBreak::IdLexicographic => candidates.sort_by(|left, right| left.1.cmp(&right.1)),
    }
    let total_candidates = candidates.len();
    candidates.truncate(config.max_candidates);
    if candidates.is_empty() {
        let reasons: Vec<String> = verdicts
            .iter()
            .map(|verdict| match &verdict.compatibility {
                Compatibility::Excluded { reasons } => reasons
                    .iter()
                    .map(|reason| format!("{}: {}", reason.code, reason.detail))
                    .collect::<Vec<_>>()
                    .join("; "),
                Compatibility::Compatible => String::new(),
            })
            .filter(|reason| !reason.is_empty())
            .collect();
        let inspection = PlanInspection {
            policy: config.policy.clone(),
            candidates: vec![],
            exclusions: exclusions.clone(),
            selected_plan_id: None,
            checks: vec![],
            budget: None,
            artifact_class: disposition_without_plan(&goal.requirements.fallback)
                .name()
                .into(),
        };
        return PlanningOutcome::NoEligible {
            reasons: if reasons.is_empty() {
                vec!["E-GOAL-201: no eligible plan".to_string()]
            } else {
                reasons
            },
            disposition: disposition_without_plan(&goal.requirements.fallback),
            inspection,
        };
    }
    // Budget check: more compatible candidates than the retention horizon
    // means the planner cannot explore everything deterministically.
    if total_candidates > config.max_candidates {
        let continuation = format!(
            "{}:resume:candidates>{}",
            config.policy, config.max_candidates
        );
        let inspection = PlanInspection {
            policy: config.policy.clone(),
            candidates: candidates.iter().map(|(_, id)| id.clone()).collect(),
            exclusions,
            selected_plan_id: None,
            checks: vec![],
            budget: Some(format!("{}candidates", verdicts.len())),
            artifact_class: disposition_exhausted(&goal.requirements.fallback)
                .name()
                .into(),
        };
        return PlanningOutcome::Exhausted {
            continuation,
            disposition: disposition_exhausted(&goal.requirements.fallback),
            inspection,
        };
    }
    // Select the first deterministic candidate and build the plan DAG.
    let (_, provider_id) = candidates[0].clone();
    let has_conversions = registry.get(&provider_id).is_some_and(|table| {
        table.capabilities.iter().any(|capability| {
            capability
                .representations
                .iter()
                .any(|rep| rep.encode_cost > 0)
        })
    });
    let (mut plan, checks) = build_plan(goal.id, &provider_id, config);
    // Node budget: a plan DAG larger than `max_nodes` is a typed refusal
    // (E-RES-100), never a silently oversized artifact.
    if plan.nodes.len() > config.max_nodes {
        let continuation = format!("{}:resume:nodes>{}", config.policy, config.max_nodes);
        let inspection = PlanInspection {
            policy: config.policy.clone(),
            candidates: candidates.iter().map(|(_, id)| id.clone()).collect(),
            exclusions,
            selected_plan_id: None,
            checks,
            budget: Some(format!(
                "E-RES-100: {} plan nodes exceed the {} node budget",
                plan.nodes.len(),
                config.max_nodes
            )),
            artifact_class: disposition_exhausted(&goal.requirements.fallback)
                .name()
                .into(),
        };
        return PlanningOutcome::Exhausted {
            continuation,
            disposition: disposition_exhausted(&goal.requirements.fallback),
            inspection,
        };
    }
    plan.excluded_candidates = exclusions
        .iter()
        .map(|(provider, code, detail)| ExcludedCandidate {
            provider: provider.clone(),
            reason: format!("{code}: {detail}"),
        })
        .collect();
    let inspection = PlanInspection {
        policy: config.policy.clone(),
        candidates: candidates.iter().map(|(_, id)| id.clone()).collect(),
        exclusions,
        selected_plan_id: Some(plan.plan_id.0.clone()),
        checks,
        budget: None,
        artifact_class: plan.artifact_class.clone(),
    };
    plan.artifact_class = disposition_for_plan(has_conversions).name().into();
    PlanningOutcome::Selected {
        inspection: PlanInspection {
            artifact_class: plan.artifact_class.clone(),
            ..inspection
        },
        plan,
    }
}

/// Builds the single-provider plan DAG deterministically.
fn build_plan(
    goal: GoalId,
    provider_id: &str,
    config: &PlannerConfig,
) -> (ResolutionPlan, Vec<String>) {
    let lower = PlanNodeId(0);
    let convert = PlanNodeId(1);
    let execute = PlanNodeId(2);
    let check = PlanNodeId(3);
    let package = PlanNodeId(4);
    let admit = PlanNodeId(5);
    let mut nodes: BTreeMap<PlanNodeId, PlanNodeDef> = BTreeMap::new();
    let provider = ProviderRef {
        id: provider_id.to_string(),
        version: "0.0.0".to_string(),
        implementation: ContentId(String::new()),
    };
    nodes.insert(
        lower,
        PlanNodeDef {
            id: lower,
            operation: PlanOperation::Lower,
            dependencies: vec![],
            provider: Some(provider.clone()),
            checks: vec![],
            fallback: None,
            budget: Some("compile:1".into()),
        },
    );
    nodes.insert(
        convert,
        PlanNodeDef {
            id: convert,
            operation: PlanOperation::Convert,
            dependencies: vec![lower],
            provider: Some(provider.clone()),
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
            dependencies: vec![convert],
            provider: Some(provider.clone()),
            checks: vec![],
            fallback: None,
            budget: Some("runtime:1".into()),
        },
    );
    nodes.insert(
        check,
        PlanNodeDef {
            id: check,
            operation: PlanOperation::Check,
            dependencies: vec![execute],
            provider: Some(provider),
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
    let identity = plan_identity_here(config, provider_id, goal, &nodes.len().to_string());
    let plan = ResolutionPlan {
        schema: SchemaId("emath.resolution-plan.v1".into()),
        plan_id: identity,
        goal,
        policy: config.policy.clone(),
        artifact_class: String::new(),
        nodes,
        root: admit,
        excluded_candidates: Vec::new(),
    };
    (plan, vec!["sir-checker.v1".to_string()])
}

/// Deterministic plan identity seed (see identity.rs for the full binding).
fn plan_identity_here(
    config: &PlannerConfig,
    provider_id: &str,
    goal: GoalId,
    node_count: &str,
) -> ContentId {
    plan_identity(
        &goal.0.to_string(),
        &config.policy,
        &[provider_id.to_string()],
        &format!("nodes-{node_count}"),
    )
}

/// Conversion cost estimate from the capability table (0 when unknown).
fn estimate_cost(registry: &ProviderRegistry, provider_id: &str) -> u8 {
    registry
        .get(provider_id)
        .and_then(|table| {
            table
                .capabilities
                .iter()
                .flat_map(|capability| capability.representations.iter())
                .map(|representation| representation.encode_cost)
                .min()
        })
        .unwrap_or(0)
}

/// Builds the excluded-candidates trace with stable reasons.
#[must_use]
pub fn excluded_trace(goal: &Goal, registry: &ProviderRegistry) -> Vec<ExcludedCandidate> {
    filter_goal(goal, registry)
        .into_iter()
        .filter_map(|verdict| match verdict.compatibility {
            Compatibility::Excluded { reasons } => Some(ExcludedCandidate {
                provider: verdict.provider,
                reason: reasons
                    .iter()
                    .map(|reason| format!("{}: {}", reason.code, reason.detail))
                    .collect::<Vec<_>>()
                    .join("; "),
            }),
            Compatibility::Compatible => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use emath_ir::{
        DeterminismPolicy, EvidenceLevel, ExactnessPolicy, FallbackPolicy, Goal, GoalId, GoalKind,
        GoalRequirements, TargetProfile,
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
                checker_bindings: vec!["sir-checker.v1".into()],
            }],
            isolation: ProviderIsolation::Static,
            lock: ProviderLock::Unlocked,
            maximum_evidence: EvidenceLevel::E2,
            deterministic: true,
        }
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
}
