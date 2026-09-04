//! Capability cells: schema, identity, and bounded admission.
//!
//! Domain mathematics enters the IR as data, never as core enum variants:
//! a cell is interned into [`crate::package::SemanticPackage::capabilities`]
//! and referenced from [`crate::expression::ExprNode::Apply`] by
//! [`CapabilityId`]. Adding `Softmax`, a field-pack op, or any future family
//! instance appends to that arena; `ExprNode`, `UnaryOp` and `BinaryOp` do
//! not grow. Core numeric vocabulary (`sin`, `exp`, …) keeps its existing
//! `UnaryOp`/`Call` spelling as the compat path until the migration
//! moves it onto cells.
//!
//! Schema `emath.capability-cell.v1`: every cell declares a closed
//! [`CellClass`], a schema version, and an explicit migration policy —
//! identity-affecting cell mutation is refused unless the migration policy
//! admits the change. Admission is bounded and typed: the closed
//! [`AdmissionRefusal`] set names every refusal; nothing silent passes.

use emath_core::QualifiedName;
use std::fmt;

mod biform;
mod model;
mod projection;

pub use biform::*;
pub use model::*;
pub use projection::*;
