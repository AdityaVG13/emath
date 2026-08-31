//! Semantic admission (Phase 1): syntax tree → typed neutral SIR.
//!
//! Orchestrates field checks, constructor/invariant admission, definition
//! typing, goal elaboration and plan construction, mirroring the public
//! `CompilerSession` surface. Everything outside the Phase 1 subset
//! receives a typed capability refusal; nothing is silently dropped.

#![forbid(unsafe_code)]

pub mod admit;
pub mod migrate;
pub mod proofs;
pub mod recognition;
pub mod session;

pub use admit::{CheckResult, SemanticTrace, TraceEntry};
pub use session::{
    CompilerPolicy, CompilerSession, EmittedAnchor, GeneratedCrate, PlanResult, SourcePackage,
};
