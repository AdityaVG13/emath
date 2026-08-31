//! Artifact corpus search over the pinned frankensearch engine (spike).
//!
//! Search adapter external to the protected set; `search` feature (default
//! OFF) pulls the pinned frankensearch git revision. Default build is
//! std-only, first-party-only.

#![forbid(unsafe_code)]

mod corpus;
mod error;

#[cfg(feature = "search")]
mod engine;

pub use corpus::{ArtifactDoc, DOC_ID_SEPARATOR, from_fs_doc_id, to_fs_doc_id};
pub use error::SearchError;

#[cfg(feature = "search")]
pub use engine::{CorpusSearch, Hit, HitSource, IndexStats, LexicalArmStats};
