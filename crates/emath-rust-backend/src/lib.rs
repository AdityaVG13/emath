//! Rust backend: EMIR → deterministic Rust via the rust-ir AST.
//!
//! Phase 1 generates one crate per admission: a struct plus constructor
//! for stateful declarations, a free function (not a method on an empty
//! struct) when there is no state and no constructors, an evaluation
//! item per `evaluate <target>` goal, and `#[test]` functions for the
//! `tests:` section. Everything is std-only, `#![forbid(unsafe_code)]`,
//! and byte-deterministic.

#![forbid(unsafe_code)]

use crate::rust_ir::ast::{
    Block, EnumDef, EnumVariant, Expr, FnDef, ImplDef, Item, Module, Param, Stmt, StructDef,
    TestDef, Ty, UnOp, Visibility, escape_ident, snake_case,
};
use crate::rust_ir::render::{render_module, render_ty};
use emath_exec_ir::{definition_order, lower_definition, lower_requirement};
use emath_ir::{ConstructionReceipt, GoalKind, SemanticPackage, TypeId, TypeNode};
use std::collections::{BTreeMap, BTreeSet};

mod codegen_helpers;
use codegen_helpers::*;
mod codegen_render;
use codegen_render::*;
mod codegen_steps;
pub mod rust_ir;

#[derive(Clone, Debug)]
pub struct BackendInput<'a> {
    pub package: &'a SemanticPackage,
    pub crate_name: String,
    pub version: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackendAnchor {
    pub label: String,
    pub file: String,
    pub start: u32,
    pub end: u32,
}

#[derive(Clone, Debug)]
pub struct BackendOutput {
    /// Relative path → file content (includes `Cargo.toml` and `src/lib.rs`).
    pub files: BTreeMap<String, String>,
    pub anchors: Vec<BackendAnchor>,
    /// Domain obligations surfaced from lowering, first-encounter order.
    pub assumptions: Vec<String>,
    /// The generated module, so the build path can run
    /// `CrateProfile::validate` (`E-CODEGEN-002`/`E-CODEGEN-004`) on the
    /// exact items that were rendered.
    pub module: Module,
    /// One construction receipt per generated constructor: the obligation
    /// matrix (class + kind per obligation) the emitted code discharges.
    pub receipts: Vec<ConstructionReceipt>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BackendError {
    NoEvaluateGoal(String),
    UnknownTarget(String),
    MissingInput(String),
    MissingGiven(String),
    UnsupportedType(String),
    MultipleConstructors(String),
    /// The artifact requests a provider/native binding that this backend
    /// cannot materialize. This is a refusal, never a generated stub.
    UnsupportedBinding {
        capability: String,
        binding: &'static str,
    },
    /// No verified Language Distribution binding is installed for the
    /// capability application.
    MissingArtifactBinding(String),
    /// A binding exists, but no artifact matches its complete verified
    /// kernel/signature/semantic-hash identity.
    StaleArtifactBinding(String),
    /// A legacy semantic EMIR operation reached the backend instead of the
    /// universal capability-application ABI.
    MissingArtifactContract(String),
    Lowering(String),
}

impl std::fmt::Display for BackendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoEvaluateGoal(name) => {
                write!(
                    f,
                    "declaration `{name}` needs an `evaluate` goal in Phase 1"
                )
            }
            Self::UnknownTarget(name) => write!(f, "evaluate target `{name}` is not a definition"),
            Self::MissingInput(name) => write!(f, "test body does not supply input `{name}`"),
            Self::MissingGiven(name) => {
                write!(
                    f,
                    "test body does not supply constructor parameter `{name}`"
                )
            }
            Self::UnsupportedType(detail) => write!(f, "unsupported type in Phase 1: {detail}"),
            Self::MultipleConstructors(name) => write!(
                f,
                "declaration `{name}` has multiple constructors (Phase 1 supports one)"
            ),
            Self::UnsupportedBinding {
                capability,
                binding,
            } => write!(
                f,
                "unsupported {binding} binding for capability `{capability}`"
            ),
            Self::MissingArtifactBinding(capability) => write!(
                f,
                "missing verified artifact binding for capability `{capability}`"
            ),
            Self::StaleArtifactBinding(capability) => write!(
                f,
                "stale or unsupported artifact binding for capability `{capability}`"
            ),
            Self::MissingArtifactContract(operation) => write!(
                f,
                "EMIR operation `{operation}` has no universal artifact contract; lower it through ApplyCapability/kernel ABI"
            ),
            Self::Lowering(detail) => write!(f, "EMIR lowering failed: {detail}"),
        }
    }
}

impl std::error::Error for BackendError {}

const DEFAULT_ERROR_TYPE: &str = "ConfigError";

mod generate;
mod ty;

// (test module relocated to tests/emath-rust-backend)
