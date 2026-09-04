//! Evidence/artifact state store (sqlite-store feature is the pinned fsqlite
//! facade behind a blocking adapter).
//!
//! Determinism class: pure sequence. No wall-clock timestamps; row order is
//! explicit via seq + claim.

#![forbid(unsafe_code)]

pub mod discovery;
pub mod evidence_plane;
pub mod materialization;
pub mod object_graph;
pub mod pack;
pub mod schema;
pub mod semantic_diff;
pub mod space;
pub mod stdlib;

pub use discovery::{DiscoveryHit, FindFilter, FindQuery};
pub use evidence_plane::{AttachedReceipt, EvidencePlane, EvidencePlaneError, EvidenceReceipt};
pub use materialization::{MaterializationRecipe, MaterializeFault, Materializer};
pub use object_graph::{
    LibraryObject, ObjectDraft, ObjectGraph, ObjectKind, Relation, RelationDraft, RelationKind,
    RelationScope, StoreGraphError,
};
pub use pack::{PackBudgets, PackEntry, PackFault, PackReader, PackWriter};
pub use semantic_diff::{
    ChangeClass, CutoffReceipt, DiffOutcome, SemanticSnapshot, classify, decide,
};
pub use space::{
    LibraryLock, MergeAction, MergeReceipt, Reconciliation, Space, SpaceError, SpacePolicy,
    SpaceSnapshot,
};
pub use stdlib::{
    StdEntry, StdMount, StdMountError, StdObject, StdReceipt, export_std_pack, mount_stdlib,
};

#[cfg(feature = "sqlite-store")]
pub mod store;

#[cfg(feature = "sqlite-store")]
pub use store::{EvidenceRow, Store, StoreError, StoreOp};
