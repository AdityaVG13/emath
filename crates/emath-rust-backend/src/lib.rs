//! Rust backend: EMIR → deterministic Rust via the rust-ir AST.
//!
//! generates one crate per admission: a struct plus constructor
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
pub mod constructor_crate;
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
                    "declaration `{name}` needs an `evaluate` goal in the current subset"
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
            Self::UnsupportedType(detail) => write!(f, "unsupported type in the current subset: {detail}"),
            Self::MultipleConstructors(name) => write!(
                f,
                "declaration `{name}` has multiple constructors (the current subset supports one)"
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

/// Emit standalone Rust for a constructor-lowered EMIR program.
/// Unresolved programs must not be marked runnable by the caller.
pub fn emit_constructor_program(
    program: &emath_exec_ir::EmirProgram,
    input_names: &[String],
) -> Result<String, BackendError> {
    use crate::codegen_render::{value_expr, InputKinds, ValueKind};
    use crate::rust_ir::render::render_expr;
    let mut kinds = InputKinds::new();
    for name in input_names {
        kinds.insert(name.clone(), ValueKind::I64);
    }
    let expr = value_expr(program, input_names, &[], &kinds)?;
    let params = input_names
        .iter()
        .map(|name| format!("{name}: i64"))
        .collect::<Vec<_>>()
        .join(", ");
    let body = render_expr(&expr);
    if contains_call_self(program) {
        let call_args = input_names.join(", ");
        Ok(format!(
            "pub fn entry({params}) -> Result<impl core::fmt::Debug, String> {{\n    fn __self({params}) -> Result<i64, String> {{\n        Ok({body})\n    }}\n    Ok(__self({call_args})?)\n}}\n"
        ))
    } else {
        Ok(format!(
            "pub fn entry({params}) -> Result<impl core::fmt::Debug, String> {{\n    Ok({body})\n}}\n"
        ))
    }
}

/// One authored record: name plus `(field, carrier signature)` rows -
/// the build-local layout for `EmathRecord_{name}`. Signatures use the
/// constructor lane's interchange forms (`Int`, `Rat`, `Record<CS>`,
/// `Vector<T>`, `Fn<A,B>`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthoredRecord {
    pub name: String,
    pub fields: Vec<(String, String)>,
}

/// Struct definitions for the authored records an entry references.
/// Field carriers render through the same kind parser as entry
/// signatures, so `Record<CS>` fields nest by name. A field the parser
/// cannot map to a concrete carrier is a refusal, not a guess.
pub fn emit_record_definitions(records: &[AuthoredRecord]) -> Result<String, BackendError> {
    use crate::codegen_render::ValueKind;
    use crate::rust_ir::ast::escape_ident;
    use crate::rust_ir::render::render_ty;
    let mut rust = String::new();
    for record in records {
        let mut fields = String::new();
        for (field, signature) in &record.fields {
            let ty = render_ty(&ValueKind::from_signature(signature).rust_ty()?);
            // Escape field names the same way the record-literal
            // render does, so `EmathRecord_X { field_: .. }` sites and
            // the struct definition agree (authored names may be Rust
            // keywords, e.g. `move`).
            fields.push_str(&format!("    pub {}: {ty},\n", escape_ident(field)));
        }
        rust.push_str(&format!(
            "#[derive(Clone, Debug, PartialEq)]\npub struct EmathRecord_{} {{\n{fields}}}\n\n",
            escape_ident(&record.name)
        ));
    }
    Ok(rust)
}

/// Emit one typed entry for a constructor-lowered function - the named
/// Rust ABI the build lane emits per function. Parameters take their
/// declared carrier signatures (`None` falls back to `Int`, the
/// numeric-lane default), authored records scope the emission's record
/// layouts, and the result carrier comes from the program's inferred
/// kind (kinds without a concrete carrier keep the `impl Debug`
/// fallback of the numeric shim). The declared `output` (authored
/// name + carrier signature, single-output functions) is the carrier
/// authority at the expression-template boundary: a CodeValue result
/// projects checked onto it - the engine's `type_admits` law (Rat
/// widens Int exactly; Int refuses a Rational by name; Bool admits
/// Bool only). Reference context is on for the whole entry, so
/// runtime refusals propagate through `Result`.
pub fn emit_constructor_entry(
    program: &emath_exec_ir::EmirProgram,
    function_name: &str,
    inputs: &[(String, Option<String>)],
    output: Option<(&str, &str)>,
    records: &[AuthoredRecord],
) -> Result<String, BackendError> {
    use crate::codegen_render::{
        program_kind, value_expr, AuthoredRecordScope, InputKinds, ReferenceScope, ValueKind,
    };
    use crate::rust_ir::ast::escape_ident;
    use crate::rust_ir::render::{render_expr, render_ty};
    let _reference = ReferenceScope::enter();
    let mut scope_records = std::collections::BTreeMap::new();
    for record in records {
        scope_records.insert(record.name.clone(), record.fields.clone());
    }
    let _records = AuthoredRecordScope::enter(scope_records);
    let mut kinds = InputKinds::new();
    let mut params = Vec::new();
    let mut self_params = Vec::new();
    let mut self_args = Vec::new();
    for (name, signature) in inputs {
        let kind = signature
            .as_deref()
            .map(ValueKind::from_signature)
            .unwrap_or(ValueKind::I64);
        params.push(format!("{name}: {}", render_ty(&kind.rust_ty()?)));
        // The recursive wrapper mirrors the public parameter exactly
        // (copy carriers, shared `Rc<dyn Fn>` closure handles, and
        // owned non-copy carriers alike), and the entry call passes
        // the parameter directly.
        self_params.push(format!("{name}: {}", render_ty(&kind.rust_ty()?)));
        self_args.push(name.clone());
        kinds.insert(name.clone(), kind);
    }
    let params = params.join(", ");
    let input_names: Vec<String> = inputs.iter().map(|(name, _)| name.clone()).collect();
    let result_kind = program_kind(program, &input_names, &[], &kinds);
    // The expression-template boundary: the union collapses onto the
    // declared carrier through the checked projection; without a
    // declared scalar output there is no carrier to collapse onto.
    let (result_ty, body) = if result_kind == ValueKind::CodeValue {
        let Some((output_name, signature)) = output else {
            return Err(BackendError::UnsupportedType(
                "an expression-template result needs a declared Int/Rat/Bool output to project its carrier".into(),
            ));
        };
        let declared = ValueKind::from_signature(signature);
        let rendered = render_expr(&value_expr(program, &input_names, &[], &kinds)?);
        let projected = match declared {
            ValueKind::I64 => format!(
                "emath_rt::code::project_i64(&({rendered}), {output_name:?})?"
            ),
            ValueKind::Rational => format!(
                "emath_rt::code::project_ratio(&({rendered}), {output_name:?})?"
            ),
            ValueKind::Bool => format!(
                "emath_rt::code::project_bool(&({rendered}), {output_name:?})?"
            ),
            _ => {
                return Err(BackendError::UnsupportedType(format!(
                    "output `{output_name}` declares `{signature}`, not a scalar the union lane can project"
                )));
            }
        };
        (render_ty(&declared.rust_ty()?), projected)
    } else {
        let result_ty = match result_kind.rust_ty() {
            Ok(ty) => render_ty(&ty),
            Err(_) => "impl core::fmt::Debug".to_string(),
        };
        let body = render_expr(&value_expr(program, &input_names, &[], &kinds)?);
        (result_ty, body)
    };
    let entry_name = escape_ident(function_name);
    // The shared-tree prologue: a tree-lane entry resets the per-run
    // mint state at its start, so every run of the same entry mints
    // identical `#scope.{id}` tokens (deterministic per run; the
    // entry is the artifact's counterpart of the VM's per-query
    // engine state).
    let prologue = if contains_tree_ops(program) {
        "    emath_rt::code_tree::reset_mint();\n"
    } else {
        ""
    };
    if contains_call_self(program) {
        let self_params = self_params.join(", ");
        let call_args = self_args.join(", ");
        Ok(format!(
            "pub fn {entry_name}({params}) -> Result<{result_ty}, String> {{\n{prologue}    fn __self({self_params}) -> Result<{result_ty}, String> {{\n        Ok({body})\n    }}\n    Ok(__self({call_args})?)\n}}\n"
        ))
    } else {
        Ok(format!(
            "pub fn {entry_name}({params}) -> Result<{result_ty}, String> {{\n{prologue}    Ok({body})\n}}\n"
        ))
    }
}

pub(crate) fn contains_call_self(program: &emath_exec_ir::EmirProgram) -> bool {
    use emath_exec_ir::EmirOp;
    program.ops.iter().any(|(op, _)| match op {
        EmirOp::CallSelf { .. } => true,
        EmirOp::Branch {
            then_body,
            else_body,
            ..
        } => contains_call_self(then_body) || contains_call_self(else_body),
        EmirOp::CallFrame { body, .. }
        | EmirOp::ProgramLiteral { body, .. }
        | EmirOp::Fold { body, .. }
        | EmirOp::Collect { body, .. } => contains_call_self(body),
        EmirOp::Iterate { body, stop, .. } => {
            contains_call_self(body) || stop.as_ref().is_some_and(contains_call_self)
        }
        _ => false,
    })
}

/// Whether a program carries the shared-tree lane (a union-lane
/// template literal, a view, or a make): such entries reference the
/// crate-level module table and reset the per-run mint state at
/// their prologue, so every run mints identical `#scope.{id}`
/// tokens.
pub(crate) fn contains_tree_ops(program: &emath_exec_ir::EmirProgram) -> bool {
    use emath_exec_ir::EmirOp;
    program.ops.iter().any(|(op, _)| match op {
        EmirOp::CodeView { .. } | EmirOp::CodeMake { .. } => true,
        EmirOp::CodeLiteral { param, body, .. } => {
            param.is_none() || contains_tree_ops(body)
        }
        EmirOp::Branch {
            then_body,
            else_body,
            ..
        } => contains_tree_ops(then_body) || contains_tree_ops(else_body),
        EmirOp::CallFrame { body, .. }
        | EmirOp::ProgramLiteral { body, .. }
        | EmirOp::Fold { body, .. }
        | EmirOp::Collect { body, .. } => contains_tree_ops(body),
        EmirOp::Iterate { body, stop, .. } => {
            contains_tree_ops(body) || stop.as_ref().is_some_and(contains_tree_ops)
        }
        _ => false,
    })
}

// (test module relocated to tests/emath-rust-backend)
