//! Resolved declaration representation (Tier 1 `emath-hir`): the open
//! declaration framework, scoped notation, and bootstrap-syntax
//! migration. Compiler glue between the syntax tree and neutral SIR.

#![forbid(unsafe_code)]

pub mod migrate;
pub mod notation;
pub mod open;

pub use migrate::{MigrationIssue, migrate_declaration};
pub use notation::{
    NotationContext, NotationEntry, NotationIssue, UseKind, check_use_arity, mount_notation,
};
pub use open::{
    Hierarchy, NotationSet, OpenAttr, OpenDecl, OpenField, OpenPayload, SectionFamily,
    SectionManifest, SectionViolation, SectionViolationReason, Spread,
};
