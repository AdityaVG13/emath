//! Custom kind schemas and restricted lowering.
//!
//! - `lang`: the schema language → shared [`emath_ir::KindSchema`].
//! - `lower`: bounded, typed lowering into core HIR with an expansion trace.
//! - `registry`: thirteen canonical schemas, deterministic writers.
//! - `load`: kind package loading with typed refusals.

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
