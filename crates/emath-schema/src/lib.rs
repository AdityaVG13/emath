//! Custom kind schemas and restricted lowering.
//!
//! - `lang`: the schema language → shared [`emath_ir::KindSchema`].
//! - `lower`: bounded, typed lowering into core HIR with an expansion trace.
//! - `registry`: thirteen canonical schemas, deterministic writers.
//! - `load`: kind package loading with typed refusals.

#![forbid(unsafe_code)]

pub mod feature_capsule;
pub mod lang;
pub mod load;
pub mod lower;
pub mod registry;

pub use feature_capsule::{
    CAPSULE_EDGE_KINDS, CLASS_RULES, CapsuleIssue, ClassRule, capsule_semantic_hash,
    parse_capsule_slot, parse_feature_capsule, parse_projection_disposition, validate_capsule,
    validate_maturity_transition,
};
pub use lang::{SchemaIssue, parse_schema_language};
pub use load::{
    ExpandTrace, KindPackage, MAX_EXPANSION_DEPTH, ResolveIssue, VersionPolicy, resolve_kind,
};
pub use lower::{
    LowerOp, LoweringIssue, MAX_LOWER_OPS, apply_lowering, is_bound, validate_lowered,
};
pub use registry::{
    REGISTRY_VERSION, SCHEMA_NAMES, SCHEMA_VERSION, SCHEMAS_VERSION, SchemaError,
    UnknownSchemaError, VERSION, all_schema_names, example_json, example_json_bytes,
    example_json_string, is_known_schema, schema_json, schema_json_bytes, schema_json_string,
    schema_names, write_example_json, write_schema_json,
};
