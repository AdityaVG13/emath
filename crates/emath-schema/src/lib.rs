//! Custom kind schemas and restricted lowering (`CRATE_MAP` Tier 1
//! `emath-schema`).
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
//! - `registry`: thirteen canonical schemas: stable version constants, deterministic JSON schema and example writers, typed unknown-name refusal.
//! - `load`: kind package loading: identity/version
//!   resolution from package locks, fails on missing kinds,
//!   checksum mismatch, incompatible schema versions and recursive
//!   expansion.

#![forbid(unsafe_code)]

pub mod lang;
pub mod load;
pub mod lower;
pub mod registry;

pub use lang::{SchemaIssue, parse_schema_language};
pub use load::{
    ExpandTrace, KindPackage, MAX_EXPANSION_DEPTH, ResolveIssue, VersionPolicy, resolve_kind,
};
pub use lower::{
    LowerOp, LoweringIssue, MAX_LOWER_OPS, apply_lowering, is_bound, validate_lowered,
};
pub use registry::{
    REGISTRY_VERSION, SCHEMA_NAMES, SCHEMA_SPEC_VERSION, SCHEMA_VERSION, SCHEMAS_VERSION,
    SchemaError, UnknownSchemaError, VERSION, all_schema_names, example_json, example_json_bytes,
    example_json_string, is_known_schema, schema_json, schema_json_bytes, schema_json_string,
    schema_names, write_example_json, write_schema_json,
};
