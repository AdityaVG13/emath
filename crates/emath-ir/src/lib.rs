//! Neutral semantic IR (SIR), goal IR (GIR), resolution plans and evidence
//! IR. Provider-free by constitution: no upstream type may appear here.

#![forbid(unsafe_code)]

pub mod canonical;
pub mod constructor;
pub mod contracts;
pub mod domains;
pub mod evidence;
pub mod expression;
pub mod goal;
pub mod ids;
pub mod kind_schema;
pub mod layers;
pub mod mig;
pub mod numeric;
pub mod operator;
pub mod package;
pub mod shapes;
pub mod type_system;
pub mod types;
pub mod units;

pub use constructor::{
    ConstructionObligation, ConstructionReceipt, Constructor, Field, ObligationClass,
    ObligationKind, TestCase, Visibility,
};
pub use contracts::{ContractRegistry, ProviderRepresentationContract};
pub use domains::{BranchConvention, Domain, DomainError, Interval, branch_point};
pub use evidence::{ClaimVerdict, EvidenceBundle, EvidenceClaim};
pub use expression::{
    BinaryOp, BinderKind, BinderVariable, ExprNode, Literal, SliceAxis, UnaryOp,
};
pub use goal::{
    CompileSpec, DeterminismPolicy, EvidenceLevel, ExactnessPolicy, ExcludedCandidate, Export,
    FallbackPolicy, Goal, GoalKind, GoalPayload, GoalRequirements, PlanNodeDef, PlanOperation,
    ProviderRef, RequestSpec, ResolutionPlan, SafetyProfile, TargetProfile, build_goal,
    native_plan, plan_identity,
};
pub use ids::{DeclarationId, EvidenceClaimId, ExprId, GoalId, PlanNodeId, TestId, TypeId};
pub use kind_schema::{
    CoreKind, KindSchema, PayloadPolicy, RepeatPolicy, SectionSchema, core_function_schema,
    core_model_schema, core_policy_schema,
};
pub use layers::IrLayer;
pub use mig::{Mig, MigEdge, MigEdgeKind, MigNode, MigNodeId, MigNodeKind};
pub use numeric::{
    NumKind, NumericBehavior, NumericError, NumericProfile, NumericType, STRICT_F64_MACHINE_EPS,
    STRICT_F64_PRECISION_BITS, cast_cost, check_error_limit, check_precision_demand,
    numeric_behavior, parse_numeric_profile, promote, tower_rows,
};
pub use operator::{DeclaredOperator, Fixity, canonical_operator};
pub use package::{
    Declaration, HostBinding, HostMethod, ImportEntry, ImportSelection, ModelResidual,
    PackageIdentity, SemanticPackage,
};
pub use shapes::{Extent, Shape, ShapeError, SparseLayout};
pub use type_system::{
    DischargeStatus, InferenceError, SchemeBody, SchemeField, TypeConstraints, TypeExpr,
    TypeScheme, TypeVar, canonical_of, render, unify,
};
pub use types::TypeNode;
pub use units::{
    Unit, UnitDim, UnitError, UnitFamily, check_compatible, lookup_unit, per_unit,
};
