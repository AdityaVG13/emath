//! emath core: identity, spans, stable diagnostics, limits, content identity.
//!
//! Tier 0 of the canonical crate map. Std only, no provider concepts.

#![forbid(unsafe_code)]

pub mod capabilities;
pub mod clifford;
pub mod codata;
pub mod combinatorics;
pub mod coordinate;
pub mod diagnostic;
pub mod dual;
pub mod game_theory;
pub mod geometry;
pub mod hash;
pub mod id;
pub mod integral;
pub mod limits;
pub mod linprog;
pub mod measure;
pub mod numtheory;
pub mod optimization;
pub mod parse;
pub mod probability;
pub mod quaternion;
pub mod sigfigs;
pub mod signal;
pub mod source;
pub mod span;
pub mod special;
pub mod statistics;
pub mod stochastic;
pub mod text;
pub mod tree;
pub mod units;
pub mod version;

pub use capabilities::{
    COMPILER_CAPABILITIES_SCHEMA_V1, CompilerCapabilities, DeferredFeature, GoalDescriptor,
    NumericModelDescriptor, SectionDescriptor, WorldClassDescriptor, compiler_capabilities,
};
pub use codata::{CodataAdjustment, CodataConstant, CodataKind};
pub use diagnostic::{Diagnostic, Diagnostics, Pedagogy, Severity};
pub use geometry::{
    Conic, E_GEOMETRY_DEGENERATE, E_GEOMETRY_NONFINITE, E_GEOMETRY_OVERFLOW, E_GEOMETRY_ZERO_DIV,
    Field, FreeVector, Line, Point, Rational, Transform, free_vector, point,
};
pub use hash::{bootstrap_content_id, content_id_of_str, fnv1a64_bytes, sha256_digest};
pub use id::{
    ArtifactId, ContentId, EvidenceId, FileId, IdentityParseError, MeaningId, MergeId, ObjectId,
    PackId, QualifiedName, RecipeId, RelationId, SchemaId, SnapshotId, SourceId, ViewId,
};
pub use integral::{
    DiscreteMeasure, E_INTEGRAL_COVERAGE, E_INTEGRAL_DOMAIN, E_INTEGRAL_KERNEL, E_INTEGRAL_MASS,
    LebesgueOn, StepFunction, fourier_transform, integrate_discrete, integrate_step,
    laplace_transform,
};
pub use measure::{
    DataAuthority, DataColumn, DataProvenance, DataSet, DistributionKind, E_MEASURE_CELL,
    E_MEASURE_EMPTY, E_MEASURE_RAGGED, E_MEASURE_UNIT, Measurement, parse_csv_dataset,
};
pub use parse::{SourceParser, register_source_parser, source_parser};
pub use signal::{
    Complex, DirectDft, DiscreteSignal, E_SIGNAL_EMPTY, E_SIGNAL_FFT_LENGTH, E_SIGNAL_RATE,
    E_SIGNAL_RATE_MISMATCH, E_SIGNAL_SAMPLE, Radix2Fft, Sampling, TransformBackend, Window,
};
pub use source::{SourceFile, SourceStore};
pub use span::Span;
pub use statistics::{
    BiasDeclaration, BiasDirection, ConsistencyDeclaration, DistributionSample, E_STATS_EMPTY,
    E_STATS_NAME, E_STATS_NONFINITE, E_STATS_PROB, E_STATS_SAMPLE_N, Estimate, EstimatorContract,
    SignificanceVerdict, VarianceKind, describe, mean, median, quantile, variance,
};
pub use stochastic::{
    ALGORITHM_PHILOX4X32_10, E_STOCH_ALGORITHM, E_STOCH_ENTROPY, E_STOCH_STREAM, Seed,
    StochasticReceipt, StreamPath, local_stream_seed, stream_value,
};
pub use text::normalize_nfc;
pub use version::{
    DeprecationStage, E_PKG_EDITION_UNKNOWN, EMATH_CANON_ENCODING_VERSION, EMATH_GRAMMAR_VERSION,
    EMATH_REFERENCE_VERSION, Edition, EditionError, VERSION_STACK,
};
