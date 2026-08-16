//! Semantic admission (Phase 1): syntax tree → typed neutral SIR.
//!
//! Orchestrates field checks, constructor/invariant admission, definition
//! typing, goal elaboration and plan construction, mirroring the frozen
//! `CompilerSession` surface from `implementation/PUBLIC_API_INVENTORY.md`.
//! Everything outside the Phase 1 subset receives a typed capability
//! refusal; nothing is silently dropped.

#![forbid(unsafe_code)]

pub mod admit;
pub mod session;
pub mod v6;

pub use admit::{CheckResult, SemanticTrace, TraceEntry};
pub use session::{
    CompilerPolicy, CompilerSession, EmittedAnchor, GeneratedCrate, PlanResult, SourcePackage,
};
