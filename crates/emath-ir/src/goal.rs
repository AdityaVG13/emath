//! Goal IR (GIR): provider-independent requested work.

use crate::ids::{EvidenceClaimId, ExprId, GoalId, PlanNodeId};
use crate::package::SemanticPackage;
use emath_core::{ContentId, SchemaId, Span, fnv1a64_bytes};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Goal {
    pub id: GoalId,
    pub kind: GoalKind,
    /// Target semantic node (output/definition name) as elaborated.
    pub target: String,
    pub expression: Option<ExprId>,
    pub requirements: GoalRequirements,
    pub source: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GoalKind {
    Evaluate,
    Differentiate,
    Integrate,
    Solve,
    Optimize,
    Simulate,
    Search,
    Prove,
    Verify,
    Compile,
    Benchmark,
    Custom(SchemaId),
}

impl GoalKind {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Evaluate => "evaluate",
            Self::Differentiate => "differentiate",
            Self::Integrate => "integrate",
            Self::Solve => "solve",
            Self::Optimize => "optimize",
            Self::Simulate => "simulate",
            Self::Search => "search",
            Self::Prove => "prove",
            Self::Verify => "verify",
            Self::Compile => "compile",
            Self::Benchmark => "benchmark",
            Self::Custom(_) => "custom",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GoalRequirements {
    pub evidence: EvidenceLevel,
    pub exactness: ExactnessPolicy,
    pub determinism: DeterminismPolicy,
    pub target: TargetProfile,
    pub fallback: FallbackPolicy,
    /// Required produce string such as `rust.library`.
    pub produce: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum EvidenceLevel {
    E0,
    E1,
    E2,
    E3,
    E4,
    E5,
}

impl EvidenceLevel {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::E0 => "E0",
            Self::E1 => "E1",
            Self::E2 => "E2",
            Self::E3 => "E3",
            Self::E4 => "E4",
            Self::E5 => "E5",
        }
    }
}

impl std::str::FromStr for EvidenceLevel {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "E0" => Ok(Self::E0),
            "E1" => Ok(Self::E1),
            "E2" => Ok(Self::E2),
            "E3" => Ok(Self::E3),
            "E4" => Ok(Self::E4),
            "E5" => Ok(Self::E5),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExactnessPolicy {
    Exact,
    Bounded { tolerance_literal: String },
    CheckedNumeric,
    Estimate,
    AnyExplicit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeterminismPolicy {
    Required,
    Preferred,
    Unspecified,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetProfile {
    pub family: String,
    pub triple: Option<String>,
    pub features: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FallbackPolicy {
    NativeOnly,
    Parametric,
    Continuation,
    Diagnostic,
    ExplicitLadder,
}

/// The `compile:` section content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompileSpec {
    pub target: String,
    pub profile: String,
    pub numeric: NumericProfile,
    pub safety: SafetyProfile,
    pub unresolved: Option<String>,
}

impl Default for CompileSpec {
    fn default() -> Self {
        Self {
            target: String::new(),
            profile: String::new(),
            numeric: NumericProfile::StrictF64,
            safety: SafetyProfile::ForbidUnsafe,
            unresolved: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumericProfile {
    StrictF64,
}

impl NumericProfile {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::StrictF64 => "strict-f64",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SafetyProfile {
    ForbidUnsafe,
}

impl SafetyProfile {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ForbidUnsafe => "forbid-unsafe",
        }
    }
}

/// Exported surface requested by the `exports:` section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Export {
    pub kind: String,
    pub name: String,
    pub is_public: bool,
}

/// Resolution plan DAG (deterministic, provider-free).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolutionPlan {
    pub schema: SchemaId,
    pub plan_id: ContentId,
    pub goal: GoalId,
    pub policy: String,
    pub artifact_class: String,
    pub nodes: BTreeMap<PlanNodeId, PlanNodeDef>,
    pub root: PlanNodeId,
    pub excluded_candidates: Vec<ExcludedCandidate>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanOperation {
    Lower,
    Convert,
    Execute,
    Check,
    Package,
    Continue,
    ReturnUnresolved,
    Admit,
}

impl PlanOperation {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Lower => "lower",
            Self::Convert => "convert",
            Self::Execute => "execute",
            Self::Check => "check",
            Self::Package => "package",
            Self::Continue => "continue",
            Self::ReturnUnresolved => "return-unresolved",
            Self::Admit => "admit",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanNodeDef {
    pub id: PlanNodeId,
    pub operation: PlanOperation,
    pub dependencies: Vec<PlanNodeId>,
    pub provider: Option<ProviderRef>,
    pub checks: Vec<EvidenceClaimId>,
    pub fallback: Option<PlanNodeId>,
    pub budget: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderRef {
    pub id: String,
    pub version: String,
    pub implementation: ContentId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExcludedCandidate {
    pub provider: String,
    pub reason: String,
}

/// Required `produce` string for the Phase 1 native export surface.
pub const PRODUCE_RUST_LIBRARY: &str = "rust.library";

/// Policy name for the deterministic native plan.
pub const POLICY: &str = "native-deterministic";

/// Schema id of the resolution plan *document* (written by
/// `emath_artifact::write_resolution_plan`).
///
/// This is deliberately **not** the plan identity preimage: the plan's
/// content id hashes a `plan:` payload built by [`plan_identity`],
/// not this schema string. The two-layer split (identity preimage vs
/// JSON `$schema`) is stable; do not merge them.
pub const PLAN_SCHEMA: &str = "emath.resolution-plan";

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

/// One elaborated `evaluate` request recovered from the `goals:` section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequestSpec {
    pub kind: String,
    pub target: String,
    pub produce: String,
    pub source: Span,
}

/// Build GIR for one request into the package arena.
pub fn build_goal(package: &mut SemanticPackage, request: &RequestSpec) -> Goal {
    let target_expr = package
        .declarations
        .iter()
        .find(|d| d.definitions.contains_key(&request.target))
        .and_then(|d| d.definitions.get(&request.target))
        .copied();
    let id = GoalId(u32::try_from(package.goals.len()).unwrap_or(u32::MAX));
    let goal = Goal {
        id,
        kind: GoalKind::Evaluate,
        target: request.target.clone(),
        expression: target_expr,
        requirements: GoalRequirements {
            evidence: EvidenceLevel::E1,
            exactness: ExactnessPolicy::Exact,
            determinism: DeterminismPolicy::Required,
            target: TargetProfile {
                family: "rust-library".into(),
                triple: None,
                features: vec![],
            },
            fallback: FallbackPolicy::NativeOnly,
            produce: if request.produce.is_empty() {
                PRODUCE_RUST_LIBRARY.to_string()
            } else {
                request.produce.clone()
            },
        },
        source: request.source,
    };
    package.push_goal(goal.clone());
    goal
}

/// Canonical plan identity: binds goal schema, policy, provider set
/// (sorted ids) and target family. The provider set is hashed sorted, so
/// a permutation of provider ids never changes a plan identity.
/// Content identity of a resolution plan.
///
/// The identity layer is independent of the JSON `$schema` layer: the
/// payload is prefixed `plan:` (see [`PLAN_SCHEMA`] for the document
/// id), so a document's identity never depends on its schema-id string.
#[must_use]
pub fn plan_identity(
    goal_canonical: &str,
    policy: &str,
    providers: &[String],
    target: &str,
) -> ContentId {
    let mut payload = String::new();
    payload.push_str("plan:");
    payload.push_str(goal_canonical);
    payload.push('\n');
    payload.push_str(policy);
    payload.push('\n');
    let mut sorted: Vec<&String> = providers.iter().collect();
    sorted.sort();
    for provider in sorted {
        payload.push_str(provider);
        payload.push('\n');
    }
    payload.push_str(target);
    ContentId(format!(
        "fnv1a64:{:016x}",
        fnv1a64_bytes(payload.as_bytes())
    ))
}

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
    // Identity via `plan_identity`: FNV over the sorted provider set
    // (plus goal/policy/target), not over raw canonical text that omitted
    // them.
    let plan_id = plan_identity(
        &goal.0.to_string(),
        POLICY,
        &["emath.native".to_string()],
        artifact_class,
    );
    ResolutionPlan {
        schema: SchemaId(PLAN_SCHEMA.into()),
        plan_id,
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

// Plan-identity tests moved to `tests/emath-ir/tests/goal.rs`.
