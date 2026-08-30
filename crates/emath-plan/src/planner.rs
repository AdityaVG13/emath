//! /06-011: deterministic planner and plan inspection.
//!
//! Ordered rules, bounded candidate retention, explicit pruning, deterministic
//! tie-breaks. Every exclusion keeps its reason; an exhausted budget yields a
//! continuation/diagnostic per the fallback policy (`E-RES-100`, `E-GOAL-201`).
//! Candidate selection runs through the resolution algebra (`algebra`).

use crate::algebra::{Lifted, QState, Step};
use crate::dispositions::{
    ArtifactDisposition, disposition_exhausted, disposition_for_plan, disposition_without_plan,
};
use crate::identity::plan_identity;
use crate::inspect::PlanInspection;
use emath_core::{ContentId, SchemaId};
use emath_ir::{
    EvidenceClaimId, ExcludedCandidate, Goal, GoalKind, PlanNodeDef, PlanNodeId,
    PlanOperation, ProviderRef, ResolutionPlan,
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
            policy: "deterministic-planner".to_string(),
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

    /// The machine inspection for this outcome (selected, empty, or exhausted).
    #[must_use]
    pub fn inspection(&self) -> &PlanInspection {
        match self {
            Self::Selected { inspection, .. }
            | Self::NoEligible { inspection, .. }
            | Self::Exhausted { inspection, .. } => inspection,
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
                // Empty reason lists must still be recorded: dropping the
                // exclusion would hide an inapplicable provider from the
                // inspection (and from the algebra's refused arms).
                let (code, detail) = match reasons.first() {
                    Some(primary) => (primary.code.to_string(), primary.detail.clone()),
                    None => (
                        "E-GOAL-201".to_string(),
                        "exclusion reported without reasons".to_string(),
                    ),
                };
                exclusions.push((verdict.provider.clone(), code, detail));
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
    // Selection through the resolution algebra: every retained candidate is
    // a fully-discharging capability step, every exclusion an inapplicable
    // one; the ordered alternative (left bias = deterministic tie-break
    // order) lifted to a total application decides between selection and
    // typed refusal.
    let mut arms: Vec<Step> = candidates
        .iter()
        .map(|(_, id)| Step::compatible(id))
        .collect();
    arms.extend(exclusions.iter().map(|(provider, code, detail)| {
        Step::refused(provider, vec![format!("{code}: {detail}")])
    }));
    let selection = Step::Alt(arms).apply_total(&QState::full());
    let application = match selection {
        Lifted::Applied(application) => application,
        Lifted::Refused { .. } => {
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
            combination: None,
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
    };
    debug_assert!(
        application.state.is_resolved(),
        "a compatible candidate discharges every facet"
    );
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
            combination: None,
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
    // The algebra's left-biased alternative selected the first deterministic
    // candidate; build its plan DAG.
    let Some(provider_id) = application.trace.first().cloned() else {
        let inspection = PlanInspection {
            policy: config.policy.clone(),
            candidates: candidates.iter().map(|(_, id)| id.clone()).collect(),
            exclusions,
            selected_plan_id: None,
            combination: None,
            checks: vec![],
            budget: None,
            artifact_class: disposition_without_plan(&goal.requirements.fallback)
                .name()
                .into(),
        };
        return PlanningOutcome::NoEligible {
            reasons: vec!["E-GOAL-201: applied selection missing provider trace".to_string()],
            disposition: disposition_without_plan(&goal.requirements.fallback),
            inspection,
        };
    };
    let has_conversions = registry.get(&provider_id).is_some_and(|table| {
        table.capabilities.iter().any(|capability| {
            capability
                .representations
                .iter()
                .any(|rep| rep.encode_cost > 0)
        })
    });
    let (mut plan, checks) = build_plan(goal, &provider_id, config);
    // Node budget: a plan DAG larger than `max_nodes` is a typed refusal
    // (E-RES-100), never a silently oversized artifact.
    if plan.nodes.len() > config.max_nodes {
        let continuation = format!("{}:resume:nodes>{}", config.policy, config.max_nodes);
        let inspection = PlanInspection {
            policy: config.policy.clone(),
            candidates: candidates.iter().map(|(_, id)| id.clone()).collect(),
            exclusions,
            selected_plan_id: None,
            combination: None,
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
        combination: Some(combination_name(goal, &provider_id)),
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

/// Deterministic `goal:solver:provider` combination name for plan
/// output (emath-9bj1, Track A3 pass 7). The goal part is the kind's
/// stable spelling; the solver part names the deterministic
/// resolution method the goal binds: the solve goal's
/// Newton-with-deterministic-bracket-fallback solver, dual forward
/// mode for derivatives, Newton-on-∇f for optimize, quadrature for
/// integrate, the fit goal's declared optimizer method when the goal
/// is a fit payload (custom kind), and the interpreter otherwise;
/// the provider part is the retained candidate provider id. The
/// fixed field order makes the name a deterministic function of
/// (goal, provider).
#[must_use]
pub fn combination_name(goal: &Goal, provider_id: &str) -> String {
    let solver = match &goal.kind {
        GoalKind::Solve => "newton-bracket",
        GoalKind::Differentiate => "dual-forward",
        GoalKind::Optimize => "newton-hessian",
        GoalKind::Integrate => "quadrature",
        GoalKind::Custom(_) if !goal.payload.method.is_empty() => goal.payload.method.as_str(),
        _ => "interpreter",
    };
    format!("{}:{solver}:{provider_id}", goal.kind.as_str())
}

/// Builds the single-provider plan DAG deterministically.
fn build_plan(
    goal: &Goal,
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
        schema: SchemaId("emath.resolution-plan".into()),
        plan_id: identity,
        goal: goal.id,
        policy: config.policy.clone(),
        artifact_class: String::new(),
        nodes,
        root: admit,
        excluded_candidates: Vec::new(),
    };
    (plan, vec!["sir-checker".to_string()])
}

/// Deterministic plan identity seed (see identity.rs for the full binding).
fn plan_identity_here(
    config: &PlannerConfig,
    provider_id: &str,
    goal: &Goal,
    node_count: &str,
) -> ContentId {
    plan_identity(
        &goal_semantic_canonical(goal),
        &config.policy,
        &[provider_id.to_string()],
        &format!("nodes-{node_count}"),
    )
}

/// Canonical semantic content of a goal for plan identity: kind, target
/// and the structured payload. Deliberately excludes the positional
/// `GoalId` (it shifts when unrelated goals are added to the package)
/// and the source span (it shifts under reformatting), so the plan id
/// changes exactly when the requested computation changes. Field order
/// is fixed; the \x1f separator cannot appear in user identifiers
/// (identifiers are alphanumeric/underscore), keeping the encoding
/// injective for practical purposes.
fn goal_semantic_canonical(goal: &Goal) -> String {
    let payload = &goal.payload;
    let mut canonical = String::new();
    canonical.push_str(goal.kind.as_str());
    // Target, then the name-list payload fields in a fixed order; each
    // list is tagged so `wrt=[a,b]` can never collide with a different
    // field assignment of the same names.
    canonical.push('\u{1}');
    canonical.push_str("target=");
    canonical.push_str(&goal.target);
    for (tag, names) in [
        ("wrt", &payload.wrt),
        ("measure", &payload.measure),
        ("parameters", &payload.parameters),
        ("model", &payload.model),
    ] {
        canonical.push('\u{1}');
        canonical.push_str(tag);
        canonical.push('=');
        for name in names {
            canonical.push_str(name);
            canonical.push(',');
        }
    }
    // Scalar payload fields in a fixed order.
    for (tag, value) in [
        (
            "order",
            payload.order.map(|order| order.to_string()),
        ),
        (
            "against",
            payload.against.clone(),
        ),
        (
            "prediction",
            Some(payload.prediction.clone()).filter(|p| !p.is_empty()),
        ),
        (
            "residual",
            Some(payload.residual.clone()).filter(|p| !p.is_empty()),
        ),
        (
            "method",
            Some(payload.method.clone()).filter(|p| !p.is_empty()),
        ),
        (
            "require_identifiability",
            Some(payload.require_identifiability.to_string())
                .filter(|_| payload.require_identifiability),
        ),
    ] {
        if let Some(value) = value {
            canonical.push('\u{1}');
            canonical.push_str(tag);
            canonical.push('=');
            canonical.push_str(&value);
        }
    }
    // Pair-valued fit fields, order-preserving (order is semantic).
    for (tag, pairs) in [
        ("initial", &payload.initial),
        ("weights", &payload.weights),
    ] {
        if !pairs.is_empty() {
            canonical.push('\u{1}');
            canonical.push_str(tag);
            canonical.push('=');
            for (name, literal) in pairs {
                canonical.push_str(name);
                canonical.push('=');
                canonical.push_str(literal);
                canonical.push(',');
            }
        }
    }
    if !payload.data.is_empty() {
        canonical.push_str("\u{1}data=");
        for (name, values) in &payload.data {
            canonical.push_str(name);
            canonical.push('=');
            for value in values {
                canonical.push_str(value);
                canonical.push(',');
            }
        }
    }
    canonical
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

// Planner tests moved to `tests/emath-plan/tests/planner.rs`.
