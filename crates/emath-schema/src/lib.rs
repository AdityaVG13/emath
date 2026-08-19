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
//! - `registry`: thirteen canonical schemas: per-id JSON Schema documents
//!   (closed-world fields from in-tree emitters; open envelope when none),
//!   deterministic writers, typed unknown-name refusal.
//! - `load`: kind package loading: identity/version
//!   resolution from package locks, fails on missing kinds,
//!   checksum mismatch, incompatible schema versions and recursive
//!   expansion.

#![forbid(unsafe_code)]

pub mod lang;
pub mod load;
pub mod lower;
pub mod registry;

pub use lang::{parse_schema_language, SchemaIssue};
pub use load::{
    resolve_kind, ExpandTrace, KindPackage, ResolveIssue, VersionPolicy, MAX_EXPANSION_DEPTH,
};
pub use lower::{
    apply_lowering, is_bound, validate_lowered, LowerOp, LoweringIssue, MAX_LOWER_OPS,
};
pub use registry::{
    all_schema_names, example_json, example_json_bytes, example_json_string, is_known_schema,
    schema_json, schema_json_bytes, schema_json_string, schema_names, write_example_json,
    write_schema_json, SchemaError, UnknownSchemaError, REGISTRY_VERSION, SCHEMAS_VERSION,
    SCHEMA_NAMES, SCHEMA_VERSION, VERSION,
};
