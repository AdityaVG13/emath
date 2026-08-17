//! Rumoca structural-model adapter: provider seam and neutral IR.
//!
//! Phase 1 implements the structural lane with in-tree native stand-ins:
//! the neutral structural/equation IR ([`structural`]), the compiler-phase
//! census ([`census`]), the dynamic-model subset contract ([`subset`]),
//! emath-to-DAE lowering ([`lower`]), DAE-plan and simulation providers
//! ([`provider`]), the semantic mapping table ([`map`]), Modelica subset
//! import ([`import`]), the MSL conformance ladder ([`conformance`]), the
//! versioned provider seam ([`seam`]) and diagnostic mapping
//! ([`diagnostics`]). No upstream Rumoca engine is consumed in Phase 1
//! (the census says so per phase); the seam is the Phase 2+ bridge to the
//! fork. No upstream type appears here; Rumoca is referenced only by
//! provider identity string.

#![forbid(unsafe_code)]

pub mod census;
pub mod conformance;
pub mod diagnostics;
pub mod import;
pub mod lower;
pub mod map;
pub mod provider;
pub mod seam;
pub mod structural;
pub mod subset;

pub use census::{PhaseKind, PhaseRecord, Stability};
pub use conformance::{ConformanceReport, FeatureResult, FeatureStatus, Tier};
pub use diagnostics::{MappedDiagnostic, ProviderDiagnostic};
pub use import::{ForeignModelDeclaration, ImportError};
pub use lower::{DaePlan, DerivativeDef, EqProvenance, LowerError};
pub use map::{ConstructMapping, MappingClass};
pub use provider::{
    SimError, SimPoint, SimulationConfig, SimulationResult, provide_dae_plan, simulate,
};
pub use seam::{AdapterSeam, ProviderVersion, SeamError};
pub use structural::{
    Component, ComponentKind, Connection, Dimensions, EqExpr, Equation, Event, InitialCondition,
    ModelIssue, StructuralModel, Unit, UnitError, VariableDecl, VariableKind,
};
pub use subset::{SubsetFeature, SubsetIssue};
