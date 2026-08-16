//! Goal IR (GIR): provider-independent requested work.

use crate::ids::{EvidenceClaimId, ExprId, GoalId, PlanNodeId};
use emath_core::{ContentId, SchemaId, Span};
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
