//! Artifact corpus search: doc-id codec and errors.
//!
//! The frankensearch engine adapter (feature `search`) was removed with the
//! never-compilable engine — the pinned fork lives in `forks/franken` and the
//! pre-cut engine is recoverable from git history. Default build is
//! std-only, first-party-only.

#![forbid(unsafe_code)]

mod corpus;
mod error;

pub use corpus::{ArtifactDoc, DOC_ID_SEPARATOR, from_fs_doc_id, to_fs_doc_id};
pub use error::SearchError;
