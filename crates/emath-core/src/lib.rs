//! emath core: identity, spans, stable diagnostics, limits, content identity.
//!
//! Tier 0 of the canonical crate map. Std only, no provider concepts.

#![forbid(unsafe_code)]

pub mod diagnostic;
mod feature_identity;
pub mod hash;
pub mod id;
pub mod limits;
pub mod parse;
mod sigfigs;
pub mod source;
pub mod span;
mod special;
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
pub use parse::{SourceParser, register_source_parser, source_parser};
pub use sigfigs::{
    E_UNIT_FMT, E_SF_MIXED_KINDS, E_SF_UNDER_REPORT, FormatSpec, FormattedQuantity, PrecisionLedger,
    PrecisionWarning, count_sig_figs, round_to_sig_figs,
};
pub use source::{SourceFile, SourceStore};
pub use span::Span;
pub use special::{
    DomainRefusal as KernelDomainRefusal, SpecialFn as KernelSpecialFn,
    evaluate_strict as evaluate_special_kernel,
};

/// Deterministic finite-sample average for generic numeric adapters.
pub fn kernel_mean(values: &[f64]) -> Result<f64, String> {
    statistics::mean(values).map(|estimate| estimate.value)
}

/// Deterministic finite-sample median for generic numeric adapters.
pub fn kernel_median(values: &[f64]) -> Result<f64, String> {
    statistics::median(values).map(|estimate| estimate.value)
}

/// Deterministic type-7 quantile for generic numeric adapters.
pub fn kernel_quantile(values: &[f64], probability: f64) -> Result<f64, String> {
    statistics::quantile(values, probability).map(|estimate| estimate.value)
}

/// Deterministic finite-sample variance; `sample` selects the n-1 denominator.
pub fn kernel_variance(values: &[f64], sample: bool) -> Result<f64, String> {
    let kind = if sample {
        statistics::VarianceKind::Sample
    } else {
        statistics::VarianceKind::Population
    };
    statistics::variance(values, kind).map(|estimate| estimate.value)
}
pub use stochastic::{Seed, StreamPath, local_stream_seed};
pub use text::normalize_nfc;
pub use units::{Quantity, QuantityKind, UnitSpec, UnitTable, seed_table};
pub use version::{
    DeprecationStage, E_PKG_EDITION_UNKNOWN, EMATH_CANON_ENCODING_VERSION, EMATH_GRAMMAR_VERSION,
    EMATH_REFERENCE_VERSION, Edition, EditionError, VERSION_STACK,
};
