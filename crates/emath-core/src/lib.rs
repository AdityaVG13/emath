//! emath core: identity, spans, stable diagnostics, limits, content identity.
//!
//! Tier 0 of the canonical crate map. Std only, no provider concepts.

#![forbid(unsafe_code)]

pub mod capabilities;
pub mod diagnostic;
pub mod hash;
pub mod id;
pub mod limits;
pub mod parse;
pub mod source;
pub mod span;
pub mod tree;

pub use capabilities::{
    COMPILER_CAPABILITIES_SCHEMA_V1, CompilerCapabilities, DeferredFeature, GoalDescriptor,
    NumericModelDescriptor, SectionDescriptor, WorldClassDescriptor, compiler_capabilities,
};
pub use diagnostic::{Diagnostic, Diagnostics, Pedagogy, Severity};
pub use hash::{bootstrap_content_id, content_id_of_str, fnv1a64_bytes, sha256_digest};
pub use id::{
    ArtifactId, ContentId, EvidenceId, FileId, IdentityParseError, MeaningId, MergeId, ObjectId,
    PackId, QualifiedName, RecipeId, RelationId, SchemaId, SnapshotId, SourceId, ViewId,
};
pub use parse::{SourceParser, register_source_parser, source_parser};
pub use source::{SourceFile, SourceStore};
pub use span::Span;
