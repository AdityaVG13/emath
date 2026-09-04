//! Programmatic model builder: construct the same semantic representation
//! (SIR package + GIR goals) that `.emath` text admission produces, without
//! a source file. Hosts and the lab use this to compose models in Rust.
//!
//! Phase 1 supports the strict-f64 subset with one declaration. The
//! constructor surface admits overloads, factories,
//! delegation, defaults, derived fields, postconditions and typed
//! errors without bypassing schema or constructor admission
//!.

#![forbid(unsafe_code)]

use emath_core::{QualifiedName, Span};
use emath_ir::constructor::Visibility;
use emath_ir::ids::DeclarationId;
use emath_ir::package::Field;
use emath_ir::{
    CompileSpec, Declaration, DeterminismPolicy, EvidenceLevel, ExactnessPolicy, ExprNode,
    FallbackPolicy, Goal, GoalKind, GoalRequirements, Literal, NumericProfile, SafetyProfile,
    SemanticPackage, TargetProfile, TypeId, TypeNode,
};
use std::collections::BTreeSet;

mod build;
mod macros;
mod model;
mod policy;

pub use build::*;
pub use macros::*;
pub use model::*;
pub use policy::*;
