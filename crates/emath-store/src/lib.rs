//! Evidence/artifact state store (frankensqlite lane, pass 1).
//!
//! Feature-gated adapter crate (`CUTOVER_PLAN.md` section 5.2 / 9.10): the
//! default build is std-only with zero third-party dependencies and only
//! first-party workspace crates for the manifest schema (emath-artifact).
//! The sqlite-store feature adds the pinned fsqlite git facade plus the
//! asupersync runtime that drives it, exposing a blocking Store over the
//! async engine.
//!
//! Determinism class: pure sequence. No wall-clock timestamps exist anywhere
//! in the schema or API; row order is explicit via seq + claim, and identical
//! writes produce identical rows (verified by tests).

#![forbid(unsafe_code)]

pub mod schema;

#[cfg(feature = "sqlite-store")]
pub mod store;

#[cfg(feature = "sqlite-store")]
pub use store::{EvidenceRow, Store, StoreError, StoreOp};
