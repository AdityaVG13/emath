//! Neutral semantic IR (SIR), goal IR (GIR), resolution plans and evidence
//! IR. Provider-free by constitution: no upstream type may appear here.

#![forbid(unsafe_code)]

pub mod canonical;
pub mod constructor;
pub mod evidence;
pub mod expression;
pub mod goal;
pub mod ids;
pub mod numeric;
pub mod package;
pub mod type_system;
pub mod types;

pub use constructor::{Constructor, Field, TestCase, Visibility};
pub use evidence::{ClaimVerdict, EvidenceBundle, EvidenceClaim};
pub use expression::{BinaryOp, BinderKind, BinderVariable, ExprNode, Literal, UnaryOp};
pub use goal::{
    CompileSpec, DeterminismPolicy, EvidenceLevel, ExactnessPolicy, ExcludedCandidate, Export,
    FallbackPolicy, Goal, GoalKind, GoalRequirements, NumericProfile, PlanNodeDef, PlanOperation,
    ProviderRef, ResolutionPlan, SafetyProfile, TargetProfile,
};
pub use ids::{DeclarationId, EvidenceClaimId, ExprId, GoalId, PlanNodeId, TestId, TypeId};
pub use numeric::{cast_cost, promote, tower_rows, NumKind, NumericError, NumericType};
pub use package::{Declaration, PackageIdentity, SemanticPackage};
pub use type_system::{
    canonical_of, render, unify, DischargeStatus, InferenceError, SchemeBody, SchemeField,
    TypeConstraints, TypeExpr, TypeScheme, TypeVar,
};
pub use types::TypeNode;
