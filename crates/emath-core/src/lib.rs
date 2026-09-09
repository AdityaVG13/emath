//! emath core: identity, spans, stable diagnostics, limits, content identity.
//!
//! Tier 0 of the canonical crate map. Std only, no provider concepts.

#![forbid(unsafe_code)]

pub mod diagnostic;
mod feature_identity;
pub mod hash;
pub mod id;
mod json;
pub mod limits;
pub mod parse;
mod sigfigs;
pub mod source;
pub mod span;
mod statistics;
mod stochastic;
pub mod text;
pub mod tree;
mod units;
mod version;

pub use diagnostic::{Diagnostic, Diagnostics, Pedagogy, Severity};
pub use feature_identity::{
    CanonicalField, DistributionHash, FeatureId, FeatureIdError, FeatureIdErrorKind, HashDomain,
    HashEnvelopeError, LegacyId, LegacyIdKind, LegacyIdMapping, LegacyMappingError,
    OperationalHash, SemanticHash,
};
pub use hash::{bootstrap_content_id, content_id_of_str, fnv1a64_bytes, sha256_digest};
pub use id::{
    ArtifactId, ContentId, EvidenceId, FileId, IdentityParseError, MeaningId, MergeId, ObjectId,
    PackId, QualifiedName, RecipeId, RelationId, SchemaId, SnapshotId, SourceId, ViewId,
};
pub use json::{json_escape, json_escape_into, json_quote, json_quote_into, JsonObject, JsonWriter};
pub use parse::{register_source_parser, source_parser, SourceParser};
pub use sigfigs::{
    count_sig_figs, round_to_sig_figs, FormatSpec, FormattedQuantity, PrecisionLedger,
    PrecisionWarning, E_SF_MIXED_KINDS, E_SF_UNDER_REPORT, E_UNIT_FMT,
};
pub use source::{SourceFile, SourceStore};
pub use span::Span;

/// Labeled finite-sample estimate: value plus method and sample size.
pub use statistics::Estimate;
pub use stochastic::{local_stream_seed, Seed, StreamPath};
pub use text::normalize_nfc;
pub use units::{seed_table, Quantity, QuantityKind, UnitSpec, UnitTable};
pub use version::{
    DeprecationStage, Edition, EditionError, EMATH_CANON_ENCODING_VERSION, EMATH_GRAMMAR_VERSION,
    EMATH_REFERENCE_VERSION, E_PKG_EDITION_UNKNOWN, VERSION_STACK,
};
