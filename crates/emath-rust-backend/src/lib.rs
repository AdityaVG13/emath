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
use codegen_helpers::{collect_var_names, emit_host_structs, field_value_kinds, add_obligations, expand_host_inputs, type_is_i64, sanitize_crate_name, sanitize_version};
mod codegen_render;
use codegen_render::{value_expr, coerce_to_ty, program_kind, program_may_fault, ValueKind};
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
            "pub fn entry({params}) -> Result<impl core::fmt::Debug, String> {{\n    let __native_entry = emath_rt::execution::Frame::enter()?;\n    fn __self({params}) -> Result<i64, String> {{\n        let __native_call = emath_rt::execution::Frame::enter()?;\n        let __native_value = {body};\n        __native_call.check()?;\n        Ok(__native_value)\n    }}\n    let __native_value = __self({call_args})?;\n    __native_entry.check()?;\n    Ok(__native_value)\n}}\n"
        ))
    } else {
        Ok(format!(
            "pub fn entry({params}) -> Result<impl core::fmt::Debug, String> {{\n    let __native_entry = emath_rt::execution::Frame::enter()?;\n    let __native_value = {body};\n    __native_entry.check()?;\n    Ok(__native_value)\n}}\n"
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
        let mut closure_fields = Vec::new();
        for (field, signature) in &record.fields {
            let kind = ValueKind::from_signature(signature);
            if matches!(kind, ValueKind::Closure { .. }) {
                closure_fields.push(field.clone());
            }
            let ty = render_ty(&kind.rust_ty()?);
            // Escape field names the same way the record-literal
            // render does, so `EmathRecord_X { field_: .. }` sites and
            // the struct definition agree (authored names may be Rust
            // keywords, e.g. `move`).
            fields.push_str(&format!("    pub {}: {ty},\n", escape_ident(field)));
        }
        if closure_fields.is_empty() {
            rust.push_str(&format!(
                "#[derive(Clone, Debug, PartialEq)]\npub struct EmathRecord_{} {{\n{fields}}}\n\n",
                crate::codegen_render::record_ident(&record.name)
            ));
            continue;
        }
        // A record with closure-carrier fields (the event module's
        // `guard`/`reset` handles) cannot derive Debug or PartialEq:
        // `dyn Fn` carries neither. The hand-impls state the VM's
        // law: data fields compare and print structurally, closure
        // fields compare by shared-handle identity (the same identity
        // fallback the VM's buffer law uses - a callable has no total
        // value equality) and print as `<closure>`.
        let name = format!("EmathRecord_{}", crate::codegen_render::record_ident(&record.name));
        let debug_fields = record
            .fields
            .iter()
            .map(|(field, _)| {
                if closure_fields.contains(field) {
                    format!(".field(\"{field}\", &\"<closure>\")")
                } else {
                    format!(".field(\"{field}\", &self.{})", escape_ident(field))
                }
            })
            .collect::<String>();
        let eq_fields = record
            .fields
            .iter()
            .map(|(field, _)| {
                if closure_fields.contains(field) {
                    format!("std::rc::Rc::ptr_eq(&self.{0}, &other.{0})", escape_ident(field))
                } else {
                    format!("self.{0} == other.{0}", escape_ident(field))
                }
            })
            .collect::<Vec<_>>()
            .join(" && ");
        rust.push_str(&format!(
            "#[derive(Clone)]\npub struct {name} {{\n{fields}}}\n\n\
             impl std::fmt::Debug for {name} {{\n    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {{\n        f.debug_struct(\"{name}\"){debug_fields}.finish()\n    }}\n}}\n\n\
             impl PartialEq for {name} {{\n    fn eq(&self, other: &Self) -> bool {{\n        {eq_fields}\n    }}\n}}\n\n",
            fields = fields,
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
/// authority at the expression-template boundary: a `CodeValue` result
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
    emit_constructor_entry_abi(program, function_name, inputs, output, records)
        .map(|(entry, _)| entry)
}

/// [`emit_constructor_entry`] plus the entry's carrier ABI (parameter
/// carriers in input order, the carrier the entry returns) - the crate
/// emission registers it so later named frames can call the entry.
pub(crate) fn emit_constructor_entry_abi(
    program: &emath_exec_ir::EmirProgram,
    function_name: &str,
    inputs: &[(String, Option<String>)],
    output: Option<(&str, &str)>,
    records: &[AuthoredRecord],
) -> Result<(String, crate::codegen_render::EntryAbi), BackendError> {
    use crate::codegen_render::{
        program_kind, value_expr, AuthoredRecordScope, InputKinds, ReferenceScope, SelfWrapperScope,
        ValueKind,
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
    let mut input_carriers = Vec::new();
    let mut params = Vec::new();
    let mut self_params = Vec::new();
    let mut self_args = Vec::new();
    let input_names: Vec<String> = inputs.iter().map(|(name, _)| name.clone()).collect();
    for (position, (name, signature)) in inputs.iter().enumerate() {
        kinds.insert(
            name.clone(),
            crate::codegen_render::sibling_entry_param(function_name, position).unwrap_or_else(
                || {
                    signature
                        .as_deref()
                        .map_or(ValueKind::I64, ValueKind::from_signature)
                },
            ),
        );
    }
    crate::codegen_render::refine_recursive_frame_kinds(program, &input_names, &[], &mut kinds);
    let mut wrapper_carriers = Vec::new();
    for (name, signature) in inputs {
        let kind = kinds.get(name).cloned().unwrap_or_else(|| signature
            .as_deref()
            .map_or(ValueKind::I64, ValueKind::from_signature));
        // The parameter identifier is the escaped name (an authored
        // Rust keyword like `ref` becomes `ref_`); the kinds map stays
        // keyed by the authored name - load rendering escapes the
        // lookup the same way.
        let ident = escape_ident(name);
        params.push(format!("{ident}: {}", render_ty(&kind.rust_ty()?)));
        // The recursive wrapper mirrors the public parameter exactly
        // (copy carriers, shared `Rc<dyn Fn>` closure handles, and
        // owned non-copy carriers alike), and the entry call passes
        // the parameter directly.
        self_params.push(format!("{ident}: {}", render_ty(&kind.rust_ty()?)));
        self_args.push(ident);
        wrapper_carriers.push(kind.clone());
        input_carriers.push(kind.clone());
    }
    let params = params.join(", ");
    let recursive = contains_call_self(program);
    let _tail = recursive.then(|| {
        crate::codegen_render::TailLoopScope::enter(program, input_names.iter().map(|name| escape_ident(name)).collect(), 0)
    });
    let result_kind = program_kind(program, &input_names, &[], &kinds);
    // The expression-template boundary: the union collapses onto the
    // declared carrier through the checked projection; without a
    // declared scalar output there is no carrier to collapse onto.
    let (result_ty, body, result_carrier) = if result_kind == ValueKind::Cell {
        let Some((_, signature)) = output else {
            return Err(BackendError::UnsupportedType("a buffer cell result requires a declared output carrier".into()));
        };
        let declared = ValueKind::from_signature(signature);
        let _wrapper = SelfWrapperScope::enter(declared.clone(), wrapper_carriers.clone());
        let rendered = render_expr(&value_expr(program, &input_names, &[], &kinds)?);
        let projected = crate::codegen_render::cell_into(&rendered, &declared)
            .ok_or_else(|| BackendError::UnsupportedType(format!("buffer output carrier `{signature}` is not emitted")))?;
        (render_ty(&declared.rust_ty()?), projected, declared)
    } else if result_kind == ValueKind::Evaluated {
        let Some((output_name, signature)) = output else {
            return Err(BackendError::UnsupportedType(
                "an evaluated value needs a declared output carrier".into(),
            ));
        };
        let declared = ValueKind::from_signature(signature);
        let _wrapper = SelfWrapperScope::enter(declared.clone(), wrapper_carriers.clone());
        let value = value_expr(program, &input_names, &[], &kinds)?;
        let projected = crate::codegen_render::numeric_boundary_value(
            &value, &result_kind, &declared, output_name,
        ).ok_or_else(|| BackendError::UnsupportedType(format!(
            "evaluated output `{output_name}` has no projection onto `{signature}`"
        )))?;
        (render_ty(&declared.rust_ty()?), render_expr(&projected), declared)
    } else if result_kind == ValueKind::CodeValue {
        let Some((output_name, signature)) = output else {
            return Err(BackendError::UnsupportedType(
                "an expression-template result needs a declared Int/Rat/Bool output to project its carrier".into(),
            ));
        };
        let declared = ValueKind::from_signature(signature);
        // The projected carrier is what the entry (and its `__self`
        // wrapper, when the body recurses) returns; nested `CallSelf`
        // sites cross from it.
        let _wrapper = SelfWrapperScope::enter(declared.clone(), wrapper_carriers.clone());
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
        (render_ty(&declared.rust_ty()?), projected, declared)
    } else {
        let result_ty = if result_kind == ValueKind::Never {
            // A body whose every path refuses never produces a value
            // (the Rust never type is experimental): the declared
            // output carrier names the unreachable result so
            // consumers type-check, and the body itself diverges.
            match output.map(|(_, signature)| ValueKind::from_signature(signature)) {
                Some(declared) if declared.rust_ty().is_ok() => {
                    render_ty(&declared.rust_ty().expect("checked above"))
                }
                _ => "()".to_string(),
            }
        } else {
            match result_kind.rust_ty() {
                Ok(ty) => render_ty(&ty),
                Err(_) => "impl core::fmt::Debug".to_string(),
            }
        };
        // The entry body's own carrier is what `__self` (when the
        // body recurses) returns; nested `CallSelf` sites read it
        // from the scope instead of re-deriving a branch-local
        // inference that cannot see the enclosing join.
        let _wrapper = SelfWrapperScope::enter(result_kind.clone(), wrapper_carriers.clone());
        let body = render_expr(&value_expr(program, &input_names, &[], &kinds)?);
        (result_ty, body, result_kind)
    };
    let abi = crate::codegen_render::EntryAbi {
        inputs: input_carriers,
        result: result_carrier,
    };
    // The shared-tree prologue: a tree-lane entry resets the per-run
    // mint state at its start, so every run of the same entry mints
    // identical `#scope.{id}` tokens (deterministic per run; the
    // entry is the artifact's counterpart of the VM's per-query
    // engine state). The public entry is the reset wrapper over a
    // body fn; in-crate calls target the body fn so a run's mint is
    // never restarted mid-run.
    let (entry_name, wrapper) = if contains_tree_ops(program) {
        let body_name = crate::codegen_render::tree_entry_body_name(function_name);
        let wrapper = format!(
            "pub fn {}({params}) -> Result<{result_ty}, String> {{\n    emath_rt::execution::run(|| {{ emath_rt::code_tree::reset_mint(); {body_name}({}) }})\n}}\n#[doc(hidden)]\n",
            escape_ident(function_name),
            input_names.iter().map(|name| escape_ident(name)).collect::<Vec<_>>().join(", ")
        );
        (body_name, wrapper)
    } else {
        (escape_ident(function_name), String::new())
    };
    if recursive {
        // Tail self calls rebind the `mut` parameters and `continue`
        // this loop (see `TailLoopScope`); other self calls recurse.
        let self_params = self_params
            .iter()
            .map(|param| format!("mut {param}"))
            .collect::<Vec<_>>()
            .join(", ");
        let call_args = self_args.join(", ");
        Ok((
            format!(
                "{wrapper}pub fn {entry_name}({params}) -> Result<{result_ty}, String> {{\n    let __native_entry = emath_rt::execution::Frame::enter()?;\n    #[allow(unused_mut, unreachable_code)]\n    fn __self({self_params}) -> Result<{result_ty}, String> {{\n        let __native_call = emath_rt::execution::Frame::enter()?;\n        loop {{ return Ok({body}); }}\n    }}\n    let __native_value = __self({call_args})?;\n    __native_entry.check()?;\n    Ok(__native_value)\n}}\n"
            ),
            abi,
        ))
    } else {
        Ok((
            format!(
                "{wrapper}pub fn {entry_name}({params}) -> Result<{result_ty}, String> {{\n    let __native_entry = emath_rt::execution::Frame::enter()?;\n    let __native_value = {body};\n    __native_entry.check()?;\n    Ok(__native_value)\n}}\n"
            ),
            abi,
        ))
    }
}

pub(crate) fn contains_call_self(program: &emath_exec_ir::EmirProgram) -> bool {
    use emath_exec_ir::EmirOp;
    program.ops.iter().any(|(op, _)| match op {
        EmirOp::CallSelf { .. } => true,
        // Branch / Fold / Collect / Iterate / ProgramLiteral bodies
        // render INLINE inside the enclosing body, so their `__self(`
        // tokens bind the enclosing wrapper. A CallFrame renders
        // self-contained (its own `__frame_self`), so recursion stops
        // here: a nested sibling's self-recursion never forces a
        // wrapper (or a `Result<!>` signature, when the frame's own
        // tail is pure refusal) on the enclosing body.
        EmirOp::Branch {
            then_body,
            else_body,
            ..
        } => contains_call_self(then_body) || contains_call_self(else_body),
        EmirOp::ProgramLiteral { body, .. }
        | EmirOp::Fold { body, .. }
        | EmirOp::Collect { body, .. } => contains_call_self(body),
        EmirOp::Iterate { body, stop, .. } => {
            contains_call_self(body) || stop.as_ref().is_some_and(contains_call_self)
        }
        // An applied function literal renders inline too: its
        // self-calls are the enclosing function's recursion.
        EmirOp::CallFrame {
            body,
            callee,
            declared,
            ..
        } if !frame_owns_self(callee.as_deref(), declared) => contains_call_self(body),
        _ => false,
    })
}

/// Whether a frame's self-calls target the frame itself: a named
/// sibling's do, and so do an applied `recur`'s (a recur frame declares
/// no inputs). An applied function literal always declares its
/// parameter; its self-calls target the enclosing function.
pub(crate) fn frame_owns_self(callee: Option<&str>, declared: &[String]) -> bool {
    callee.is_some() || declared.is_empty()
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
        // The binder half:
        // open consumes the per-run minted-ids state (the witness
        // check), and the call-form bind re-stamps against the module
        // table.
        EmirOp::CodeOpen { .. } | EmirOp::CodeBind { .. } => true,
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

/// Whether a lowered program carries the definition-table lane (a
/// `quote.body` unfold): such entries reference the crate-level
/// definition table static (the module's function bodies as data).
pub(crate) fn contains_body_ops(program: &emath_exec_ir::EmirProgram) -> bool {
    use emath_exec_ir::EmirOp;
    program.ops.iter().any(|(op, _)| match op {
        EmirOp::CodeBody { .. } => true,
        EmirOp::CodeLiteral { param, body, .. } => {
            param.is_none() || contains_body_ops(body)
        }
        EmirOp::Branch {
            then_body,
            else_body,
            ..
        } => contains_body_ops(then_body) || contains_body_ops(else_body),
        EmirOp::CallFrame { body, .. }
        | EmirOp::ProgramLiteral { body, .. }
        | EmirOp::Fold { body, .. }
        | EmirOp::Collect { body, .. } => contains_body_ops(body),
        EmirOp::Iterate { body, stop, .. } => {
            contains_body_ops(body) || stop.as_ref().is_some_and(contains_body_ops)
        }
        _ => false,
    })
}

// (test module relocated to tests/emath-rust-backend)
