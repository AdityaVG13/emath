//! Custom kind schemas and restricted lowering (`CRATE_MAP` Tier 1
//! `emath-schema`, ).
//!
//! - `lang`: the schema language: required/optional/
//!   repeatable sections, payload policies, defaults, predicates and
//!   stable diagnostics. Output is the shared
//!   [`emath_ir::KindSchema`] the compiler and builder both admit
//!   against.
//! - `lower`: restricted lowering: bounded, typed
//!   transformations from custom sections into core HIR. Invalid
//!   lowering cannot publish HIR; every application keeps an
//!   expansion trace.
//! - `load`: kind package loading: identity/version
//!   resolution from package locks, fails on missing kinds,
//!   checksum mismatch, incompatible schema versions and recursive
//!   expansion.

#![forbid(unsafe_code)]

pub mod lang;
pub mod load;
pub mod lower;

pub use lang::{parse_schema_language, SchemaIssue};
pub use load::{
    resolve_kind, ExpandTrace, KindPackage, ResolveIssue, VersionPolicy, MAX_EXPANSION_DEPTH,
};
pub use lower::{
    apply_lowering, is_bound, validate_lowered, LowerOp, LoweringIssue, MAX_LOWER_OPS,
};
