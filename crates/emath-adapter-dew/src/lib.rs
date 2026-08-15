//! Dew expression and code-generation adapter (V5 Phase 2).
//!
//! Adapts Dew's mature expression/code-generation machinery without letting
//! Dew types enter emath's durable IRs. This crate is the adapter seam: it
//! owns a mirror of the Dew scalar surface, the exact-strict-Float64
//! mapping (equivalence relation and typed refusals), source mapping from
//! SIR/EMIR through Dew nodes to generated symbols, a differential oracle,
//! and the optimization-evidence policy for promoted rewrites.
//!
//! Phase 1 scope: the fork itself is not
//! vendored; the crate freezes the adapter-facing contract so a vendored
//! backend can be dropped in later without touching emath-core/emath-ir.

#![forbid(unsafe_code)]

pub mod census;
pub mod mapping;
pub mod mirror;
pub mod oracle;
pub mod policy;

pub use census::{BackendTarget, DewCapabilityCensus, ForkPatchPolicy, PatchCategory};
pub use mapping::{MappingResult, SirNodePosition, SourceMapper, UnsupportedKind};
pub use mirror::{
    DewMirrorProgram, DewOp, DewProgramError, MapResult, SIMPLE_UNARY_MAP, STRICT_BINARY_MAP,
};
pub use oracle::{
    ComparePolicy, DifferentialOracle, OracleCase, OracleOutcome, OracleReport, F64_BIT_MODE,
};
pub use policy::{EvidenceTier, OptimizationEvidencePolicy, RewriteProposal};
