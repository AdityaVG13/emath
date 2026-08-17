//! Deterministic resolution planning (Phase 1 bootstrap + Phase 6 planner
//! machinery).
//!
//! The canonical v1 native plan constructor (`native_plan`) moved down to
//! `emath-ir` (the crate that owns `ResolutionPlan` and every plan-node
//! type); this crate hosts the provider-facing planner surface:
//! decomposition rules, representation planning, fallback graphs, provider
//! lifting, total dispositions, inspection and the plan identity/cache.
//! No external providers are installed in Phase 1.

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
    DecompositionRule, SubgoalDag, SubgoalNode, decompose, requirements_preserved,
};
pub use dispositions::{
    ArtifactDisposition, disposition_exhausted, disposition_for_plan, disposition_without_plan,
};
pub use fallback::{FallbackGraph, FallbackNode};
pub use identity::{PlanCache, ProviderFingerprint, plan_identity, provider_set_fingerprint};
pub use inspect::PlanInspection;
pub use lifting::{LiftedMethod, ProviderTraitSpec, emit_provider_trait, lift_missing};
pub use planner::{PlannerConfig, PlanningOutcome, TieBreak, plan};
pub use representations::{Conversion, ConversionNode, RepresentationError, find_conversion_path};
