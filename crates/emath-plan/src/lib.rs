//! Deterministic resolution planning (Phase 1 bootstrap + Phase 6 planner
//! machinery).
//!
//! Provider-facing planner surface: decomposition, representation planning,
//! fallback graphs, provider lifting, dispositions, inspection, and the plan
//! identity/cache. The native plan constructor lives in `emath-ir`.

#![forbid(unsafe_code)]

pub mod algebra;
pub mod decompose;
pub mod dispositions;
pub mod fallback;
pub mod identity;
pub mod inspect;
pub mod lifting;
pub mod planner;
pub mod registry_helpers;
pub mod representations;

pub use algebra::{Application, Facet, Lifted, QState, Step, fallback, parallel, serial};
pub use decompose::{
    DecompositionRule, SubgoalDag, SubgoalNode, decompose, requirements_preserved,
};
pub use dispositions::{
    ArtifactDisposition, disposition_exhausted, disposition_for_plan, disposition_without_plan,
};
pub use fallback::{FallbackGraph, FallbackNode};
pub use identity::{PlanCache, ProviderFingerprint, plan_identity, provider_set_fingerprint};
pub use inspect::PlanInspection;
pub use lifting::{LiftedMethod, ProviderTraitSpec, emit_provider_trait, lift_missing};
pub use planner::{PlannerConfig, PlanningOutcome, TieBreak, combination_name, plan};
pub use representations::{Conversion, ConversionNode, RepresentationError, find_conversion_path};
