//! Typed error model for emath-search.
//!
//! No E-* codes are introduced; ERROR_CODES.md is untouched. Engine failures
//! are mapped onto this enum (frankensearch's `SearchError` stays behind the
//! facade; see `engine.rs`).

use std::fmt;

/// Typed, actionable search crate errors.
#[derive(Debug)]
pub enum SearchError {
    /// Caller-supplied argument is invalid (empty id/kind, separator in a
    /// composite id, empty query, empty corpus, zero result limit, ...).
    InvalidArgument { field: &'static str, reason: String },
    /// The index directory could not be opened (missing index, corrupt
    /// artifacts, unreadable layout).
    Open { path: String, reason: String },
    /// An index build/indexing operation failed, with an aggregate report.
    Build { report: String },
    /// A search query failed at the engine or worker boundary.
    Query { reason: String },
    /// The operation requires a built/open index that is not present.
    NotReady { reason: String },
    /// The engine worker terminated (channel closed) before replying.
    WorkerDown,
}

impl fmt::Display for SearchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SearchError::InvalidArgument { field, reason } => {
                write!(f, "invalid argument `{field}`: {reason}")
            }
            SearchError::Open { path, reason } => write!(f, "open index at {path}: {reason}"),
            SearchError::Build { report } => write!(f, "index build failed: {report}"),
            SearchError::Query { reason } => write!(f, "search query failed: {reason}"),
            SearchError::NotReady { reason } => {
                write!(f, "index not ready: {reason}")
            }
            SearchError::WorkerDown => write!(f, "search worker terminated unexpectedly"),
        }
    }
}

impl std::error::Error for SearchError {}
