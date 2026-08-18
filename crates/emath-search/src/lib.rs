//! Artifact corpus search over the pinned frankensearch engine (spike).
//!
//! Layer: search adapter external to the protected set — it may import
//! frankensearch under the `search` feature; emath-core / emath-ir /
//! emath-goal / emath-plan / emath-artifact / emath-checker stay Franken-free.
//! See CONTRACT.md for the full contract and no-claim boundaries.
//!
//! Default build: std-only, first-party-only, zero third-party dependencies.
//! Feature `search` (default OFF) pulls the pinned frankensearch git revision
//! plus the asupersync runtime instance frankensearch itself resolves.

#![forbid(unsafe_code)]

mod corpus;
mod error;

#[cfg(feature = "search")]
mod engine;

pub use corpus::{ArtifactDoc, DOC_ID_SEPARATOR, from_fs_doc_id, to_fs_doc_id};
pub use error::SearchError;

#[cfg(feature = "search")]
pub use engine::{CorpusSearch, Hit, HitSource, IndexStats, LexicalArmStats};
