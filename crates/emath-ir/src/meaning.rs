//! Canonical admitted meaning identity.
//!
//! Meaning identity is deliberately narrower than source/content identity:
//! presentation, declaration/local/binder names, tests, evidence attachments,
//! and host bindings do not enter the preimage. Admitted types, expressions,
//! goals, numeric policy, unresolved-meaning state, and dependency meanings do.

use crate::constructor::{Field, Visibility};
use crate::expression::{BinderKind, ExprNode, Literal, SliceAxis};
use crate::goal::{DeterminismPolicy, ExactnessPolicy, FallbackPolicy, GoalKind};
use crate::ids::{ExprId, GoalId, TypeId};
use crate::package::{Declaration, ImportSelection, SemanticPackage};
use crate::types::TypeNode;
use emath_core::MeaningId;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// Version of the canonical admitted-meaning rules.
pub const MEANING_CANONICAL_SCHEMA_V1: &str = "emath.meaning.canonical.v1";

/// Malformed or internally inconsistent SIR cannot be assigned a `MeaningID`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MeaningError {
    MissingExpr(ExprId),
    MissingGoal(GoalId),
    MissingType(TypeId),
    CyclicExpr(ExprId),
    CyclicDefinition(String),
    /// Variant restored with the Apply encoding arm after an accidental
    /// working-tree revert. Tag 17 is unique and the encoding is keyed
    /// on the cell name, not the arena slot; `canonical_meaning_bytes`
    /// enforces the package integrity gate that rejects dangling
    /// capability references.
    MissingCapability(crate::CapabilityId),
}

impl fmt::Display for MeaningError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingExpr(id) => write!(formatter, "missing SIR expression {}", id.0),
            Self::MissingGoal(id) => write!(formatter, "missing SIR goal {}", id.0),
            Self::MissingType(id) => write!(formatter, "missing SIR type {}", id.0),
            Self::CyclicExpr(id) => write!(formatter, "cyclic SIR expression {}", id.0),
            Self::CyclicDefinition(name) => {
                write!(formatter, "cyclic admitted definition `{name}`")
            }
            Self::MissingCapability(id) => write!(
                formatter,
                "capability cell id {} is not interned in the package",
                id.index()
            ),
        }
    }
}

impl std::error::Error for MeaningError {}

mod ctx;
mod encode;

pub use ctx::*;
pub use encode::*;
