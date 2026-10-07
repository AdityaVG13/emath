//! Language-owned records: kind schemas and Feature Capsules.
//!
//! - `kind_schema`: the core declaration-kind section schemas.
//! - `capsule_record` / `feature_capsule`: Feature Capsule records, parsing,
//!   validation and semantic-hash rewriting.

#![forbid(unsafe_code)]

pub mod capsule_record;
pub mod feature_capsule;
pub mod kind_schema;

pub use capsule_record::{
    CapsuleEdge, CapsuleProjection, CapsuleRecordError, CapsuleSlot, FEATURE_CAPSULE_SCHEMA,
    FeatureCapsule, FeatureClass, LegacyCellMapping, Maturity, ProjectionDisposition,
};
pub use kind_schema::{
    CoreKind, KindSchema, PayloadPolicy, RepeatPolicy, SectionSchema, core_function_schema,
    core_object_schema, core_query_schema, payload_allows,
};
pub use feature_capsule::{
    CAPSULE_EDGE_KINDS, CLASS_RULES, CapsuleIssue, ClassRule, SemanticHashRewrite,
    capsule_documents, capsule_semantic_hash, parse_capsule_slot, parse_feature_capsule,
    parse_projection_disposition, rewrite_declared_semantic_hashes, validate_capsule,
    validate_maturity_transition,
};
