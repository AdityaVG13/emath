//! Resolved declaration representation (`CRATE_MAP` Tier 1 `emath-hir`):
//! the open declaration framework ( core), scoped notation
//! and bootstrap-syntax migration.
//!
//! This crate is the compiler glue between the syntax tree and the
//! neutral SIR: it collects section families, attributes, generics,
//! documentation and extension payloads with provenance into a `Hir`,
//! mounts scoped notation on it, and can migrate a bootstrap-era
//! declaration into the open framework under its bootstrap schema.

#![forbid(unsafe_code)]

pub mod migrate;
pub mod notation;
pub mod open;

pub use migrate::{migrate_declaration, MigrationIssue};
pub use notation::{
    check_use_arity, mount_notation, NotationContext, NotationEntry, NotationIssue, UseKind,
};
pub use open::{
    Hierarchy, NotationSet, OpenAttr, OpenDecl, OpenField, OpenPayload, SectionFamily,
    SectionManifest, SectionViolation, SectionViolationReason, Spread,
};
