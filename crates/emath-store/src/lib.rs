//! Evidence/artifact state store (sqlite-store feature is the pinned fsqlite
//! facade behind a blocking adapter).
//!
//! Determinism class: pure sequence. No wall-clock timestamps; row order is
//! explicit via seq + claim.

#![forbid(unsafe_code)]

pub mod object_graph;
pub mod schema;
pub mod space;

pub use object_graph::{
    LibraryObject, ObjectDraft, ObjectGraph, ObjectKind, Relation, RelationDraft, RelationKind,
    RelationScope, StoreGraphError,
};
pub use space::{
    LibraryLock, MergeAction, MergeReceipt, Reconciliation, Space, SpaceError, SpacePolicy,
    SpaceSnapshot,
};

#[cfg(feature = "sqlite-store")]
pub mod store;

#[cfg(feature = "sqlite-store")]
pub use store::{EvidenceRow, Store, StoreError, StoreOp};
