use std::collections::{BTreeMap, HashMap};

use crate::rust_ir::ast::{BinOp, Block, Expr, Stmt, Ty, UnOp, escape_ident};
use crate::rust_ir::render::render_expr;
use emath_exec_ir::optimize::{is_total, operand_registers};
use emath_exec_ir::{EmirOp, EmirProgram, EmirSliceAxis, EmirValue, FoldCombine};

use crate::BackendError;
use crate::codegen_helpers::comparison;

mod op_arith;
mod op_collections;
mod op_data;
mod op_flow;

use op_arith::op_arith_exprs;
use op_collections::op_collection_exprs;
use op_data::op_data_exprs;
use op_flow::{op_flow_exprs, authored_control_expr};

mod carrier;
mod flat;
mod kinds;
mod rtcalls;

pub(crate) use carrier::*;
pub(crate) use flat::*;
pub(crate) use kinds::*;
pub(crate) use rtcalls::*;

pub(crate) fn op_expr(
    op: &EmirOp,
    program: &EmirProgram,
    names: &[String],
    states: &[String],
    input_kinds: &InputKinds,
) -> Result<Expr, BackendError> {
    match op {
        EmirOp::CallFrame { body, inputs, state } => {
            let kinds = value_kinds(program, names, states, input_kinds);
            literal_frame_expr(body, inputs, state, program, &kinds)
        }
        EmirOp::SameDenseShape(..) | EmirOp::DenseValues(_) | EmirOp::DenseRepack { .. } | EmirOp::ToF64(_)
        | EmirOp::DenseLayout(_) | EmirOp::VectorSlice { .. } | EmirOp::VectorConcat(_) | EmirOp::F64SortTotal(_) => {
            let kinds = value_kinds(program, names, states, input_kinds);
            op_collection_exprs(op, program, &kinds)
        }
        EmirOp::Branch { .. } | EmirOp::Iterate { .. } | EmirOp::Collect { .. } | EmirOp::Refuse(_) | EmirOp::RefuseValue(_) => authored_control_expr(op, program, names, states, input_kinds),
        EmirOp::ListCreate(elements) => {
            let kinds = value_kinds(program, names, states, input_kinds);
            Ok(Expr::Macro { name: "vec".into(), args: elements.iter().map(|value| owned_operand(program, *value, &kinds)).collect() })
        }
        EmirOp::RecordField { record, field } => {
            let value = Expr::Field { receiver: Box::new(operand(program, *record)), field: escape_ident(field) };
            let kinds = value_kinds(program, names, states, input_kinds);
            if kind_of_op(op, &kinds, names, states, input_kinds).is_copy() { Ok(value) } else { Ok(Expr::Raw(format!("&{}", render_expr(&value)))) }
        }
        EmirOp::ToInt(value) => {
            let kinds = value_kinds(program, names, states, input_kinds);
            checked_integer_operand(program, *value, &kinds)
        }
        EmirOp::IntegerQuotient(left, right) => {
            let kinds = value_kinds(program, names, states, input_kinds);
            if kind_at(&kinds, *left) != ValueKind::I64 || kind_at(&kinds, *right) != ValueKind::I64 {
                return Err(BackendError::UnsupportedType("quotient requires two Int operands".into()));
            }
            Ok(map_runtime_result(format!("{}.checked_div({}).ok_or(\"E-INTEGER-QUOTIENT: zero divisor or i64 overflow\")", render_expr(&operand(program, *left)), render_expr(&operand(program, *right)))))
        }
        EmirOp::SameBits(left, right) => {
            let kinds = value_kinds(program, names, states, input_kinds);
            Ok(Expr::Raw(format!("({}).to_bits() == ({}).to_bits()", render_expr(&typed_operand(program, *left, ValueKind::F64, &kinds)), render_expr(&typed_operand(program, *right, ValueKind::F64, &kinds)))))
        }
        EmirOp::ConstF64(bits) => Ok(Expr::F64(*bits)),
        EmirOp::ConstI64(value) => Ok(Expr::Int(*value)),
        EmirOp::ConstBigInt(digits) => Ok(Expr::Raw(format!(
            "emath_rt::UBig::parse_decimal(\"{digits}\").expect(\"const-bigint digits\")"
        ))),
        EmirOp::ConstText(value) => Ok(Expr::Str(value.clone())),
        EmirOp::ConstComplex(..)
        | EmirOp::ConstBool(..)
        | EmirOp::LoadInput(..)
        | EmirOp::LoadState(..)
        | EmirOp::FormatText { .. }
        | EmirOp::SeriesCreate { .. }
        | EmirOp::SeriesSample { .. }
        | EmirOp::SetCreate { .. }
        | EmirOp::SetContains { .. }
        | EmirOp::RecordCreate { .. } => op_data_exprs(op, program, names, states, input_kinds),
        EmirOp::F64Add(..)
        | EmirOp::F64Sub(..)
        | EmirOp::F64Mul(..)
        | EmirOp::F64Div(..)
        | EmirOp::F64Pow(..)
        | EmirOp::Neg(..)
        | EmirOp::UnaryBuiltin(..)
        | EmirOp::BinaryBuiltin(..)
        | EmirOp::Lt(..)
        | EmirOp::Le(..)
        | EmirOp::Gt(..)
        | EmirOp::Ge(..)
        | EmirOp::Eq(..)
        | EmirOp::Ne(..)
        | EmirOp::And(..)
        | EmirOp::Or(..)
        | EmirOp::Imply(..)
        | EmirOp::Iff(..)
        | EmirOp::Not(..)
        | EmirOp::IsFinite(..) => op_arith_exprs(
            op,
            program,
            &value_kinds(program, names, states, input_kinds),
        ),
        EmirOp::Select { .. }
        | EmirOp::VectorCreate(..)
        | EmirOp::VectorLength(..)
        | EmirOp::MatrixCreate { .. }
        | EmirOp::MatrixRows(_)
        | EmirOp::MatrixCols(_)
        | EmirOp::MatrixPack { .. }
        | EmirOp::TensorShape(_)
        | EmirOp::F64Exp2(_)
        | EmirOp::F64PowI(..)
        | EmirOp::TextTrim(_)
        | EmirOp::TextLength(_)
        | EmirOp::IndexText(_)
        | EmirOp::TextByte(..)
        | EmirOp::FormatScientific(..)
        | EmirOp::ParseF64(_)
        | EmirOp::TensorPack { .. }
        | EmirOp::DenseIndex { .. }
        | EmirOp::TensorCreate { .. }
        | EmirOp::VectorIndex { .. }
        | EmirOp::MatrixIndex { .. }
        | EmirOp::TensorIndex { .. }
        | EmirOp::TensorSlice { .. } => op_collection_exprs(
            op,
            program,
            &value_kinds(program, names, states, input_kinds),
        ),
        EmirOp::Fold { .. }
        | EmirOp::OptionSome(..)
        | EmirOp::OptionNone
        | EmirOp::OptionIsSome(..)
        | EmirOp::OptionUnwrapOr(..)
        | EmirOp::ResultOk(..)
        | EmirOp::ResultErr(..)
        | EmirOp::ResultIsOk(..)
        | EmirOp::ResultUnwrapOr(..)
        | EmirOp::ResultErrorOf(..) => op_flow_exprs(
            op,
            program,
            names,
            states,
            input_kinds,
            &value_kinds(program, names, states, input_kinds),
        ),
        EmirOp::ApplyCapability {
            capability,
            class,
            args,
        } => capability_artifact_expr(
            capability,
            *class,
            args,
            program,
            &value_kinds(program, names, states, input_kinds),
        ),
        EmirOp::ToF64Vector(sequence) => {
            let kinds = value_kinds(program, names, states, input_kinds);
            if kind_at(&kinds, *sequence) != ValueKind::Vector(Box::new(ValueKind::F64)) {
                return Err(BackendError::UnsupportedType("vector packing requires Float64 elements".into()));
            }
            Ok(owned_operand(program, *sequence, &kinds))
        }
        EmirOp::CallProgram { program: callable, inputs }
        | EmirOp::CallScalarProgram { program: callable, inputs }
        | EmirOp::CallRealProgram { program: callable, inputs }
        | EmirOp::TryCallRealProgram { program: callable, inputs } => {
            let kinds = value_kinds(program, names, states, input_kinds);
            if kind_at(&kinds, *callable) != ValueKind::Program
                || kind_at(&kinds, *inputs) != ValueKind::Vector(Box::new(ValueKind::F64))
            {
                return Err(BackendError::UnsupportedType("numeric call requires Program and Vector<Float64>".into()));
            }
            let call = format!("({})(&{})", render_expr(&operand(program, *callable)), render_expr(&operand(program, *inputs)));
            Ok(match op {
                EmirOp::CallProgram { .. } => map_runtime_result(format!("{call}.and_then(emath_rt::NumericProgramResult::into_vector)")),
                EmirOp::CallScalarProgram { .. } => map_runtime_result(format!("{call}.and_then(emath_rt::NumericProgramResult::into_scalar)")),
                EmirOp::CallRealProgram { .. } => map_runtime_result(format!("{call}.and_then(emath_rt::NumericProgramResult::into_real)")),
                _ => Expr::Raw(format!("{call}.and_then(emath_rt::NumericProgramResult::into_real)")),
            })
        }
        EmirOp::ProgramLiteral { body, captures, vector_input } => {
            if body.state_count != 0 {
                return Err(BackendError::UnsupportedType("program literal must be closed over state".into()));
            }
            let parameters = (0..body.input_count).map(|index| format!("__program_arg_{index}")).collect::<Vec<_>>();
            let explicit = usize::from(body.input_count).checked_sub(captures.len())
                .ok_or_else(|| BackendError::UnsupportedType("program captures exceed input count".into()))?;
            if *vector_input && explicit != 1 {
                return Err(BackendError::UnsupportedType("vector program requires exactly one explicit input".into()));
            }
            let kinds = value_kinds(program, names, states, input_kinds);
            let mut inputs = InputKinds::new();
            let mut capture_bindings = String::new();
            let mut bindings = String::new();
            for (index, name) in parameters.iter().enumerate() {
                let kind = if index < explicit {
                    if *vector_input {
                        bindings.push_str(&format!("let {name} = __program_inputs.to_vec(); "));
                        ValueKind::Vector(Box::new(ValueKind::F64))
                    } else {
                        bindings.push_str(&format!("let {name} = __program_inputs[{index}]; "));
                        ValueKind::F64
                    }
                } else {
                    let argument = captures[index - explicit];
                    let kind = kind_at(&kinds, argument);
                    let value = render_expr(&owned_operand(program, argument, &kinds));
                    capture_bindings.push_str(&format!("let __program_capture_{index} = {value}; "));
                    let borrow = if kind.is_copy() { "" } else { "&" };
                    bindings.push_str(&format!("let {name} = {borrow}__program_capture_{index}; "));
                    kind
                };
                inputs.insert(name.clone(), kind);
            }
            let _reference = ReferenceScope::enter();
            let body_code = render_expr(&value_expr(body, &parameters, &[], &inputs)?);
            let result = match program_kind(body, &parameters, &[], &inputs) {
                ValueKind::F64 => format!("emath_rt::NumericProgramResult::Scalar({body_code})"),
                ValueKind::I64 => format!("emath_rt::NumericProgramResult::Integer({body_code})"),
                ValueKind::Never => body_code,
                ValueKind::Vector(element) if *element == ValueKind::F64 => format!("emath_rt::NumericProgramResult::Vector({body_code})"),
                _ => return Err(BackendError::UnsupportedType("numeric program must return Float64, Int, or Vector<Float64>".into())),
            };
            let arity_guard = if *vector_input { String::new() } else { format!("if __program_inputs.len() != {explicit} {{ return Err(String::from(\"call-program: argument count mismatch\")); }} ") };
            Ok(Expr::Raw(format!("{{ {capture_bindings} let __program: std::sync::Arc<dyn Fn(&[f64]) -> Result<emath_rt::NumericProgramResult, String>> = std::sync::Arc::new(move |__program_inputs: &[f64]| {{ {arity_guard} {bindings} Ok({result}) }}); __program }}")))
        }
        EmirOp::VectorMap { .. }
        | EmirOp::VectorMapScalar { .. }
        | EmirOp::VectorReduce { .. }
        | EmirOp::VectorAllFinite(_) => {
            Err(BackendError::MissingArtifactContract(op.name().to_string()))
        }
    }
}

/// Resolve the universal capability seam from artifact-contract data only.
/// Capability names remain opaque and are never used for backend dispatch.
fn capability_artifact_expr(
    capability: &str,
    class: emath_exec_ir::CellClass,
    args: &[EmirValue],
    program: &EmirProgram,
    kinds: &[ValueKind],
) -> Result<Expr, BackendError> {
    match class {
        emath_exec_ir::CellClass::Provider => Err(BackendError::UnsupportedBinding {
            capability: capability.to_string(),
            binding: "provider",
        }),
        emath_exec_ir::CellClass::Intrinsic => Err(BackendError::UnsupportedBinding {
            capability: capability.to_string(),
            binding: "native",
        }),
        emath_exec_ir::CellClass::Pure => {
            if emath_exec_ir::native_kernel::native_kernel(capability).is_none() {
                if let Some(cell) =
                    emath_exec_ir::native_kernel::installed_reference_cell(capability)
                {
                    if !cell.admits_arity(args.len())
                        || !cell.guards.is_empty()
                        || cell.result_guard.is_some()
                    {
                        return Err(BackendError::MissingArtifactContract(
                            capability.to_string(),
                        ));
                    }
                    let names = (0..cell.params.len())
                        .map(|index| format!("__reference_arg_{index}"))
                        .collect::<Vec<_>>();
                    let mut inputs = names.iter().zip(args)
                        .map(|(name, argument)| (name.clone(), kind_at(kinds, *argument)))
                        .collect();
                    let _reference = ReferenceScope::enter();
                    let mut source = String::from("{ ");
                    for (name, argument) in names.iter().zip(args) {
                        let kind = kind_at(kinds, *argument);
                        let value = render_expr(&operand(program, *argument));
                        if kind.is_copy() {
                            source.push_str(&format!("let {name} = {value}; "));
                        } else {
                            let ty = crate::rust_ir::render::render_ty(&kind.borrowed_rust_ty()?);
                            source.push_str(&format!("let {name}: &{ty} = &{value}; "));
                        }
                    }
                    let first_default = cell.params.len() - cell.defaults.len();
                    for index in args.len()..names.len() {
                        let default = &cell.defaults[index - first_default];
                        let code = render_expr(&value_expr(default, &names[..index], &[], &inputs)?);
                        let kind = program_kind(default, &names[..index], &[], &inputs);
                        source.push_str(&format!("let {} = {code}; ", names[index]));
                        inputs.insert(names[index].clone(), kind);
                    }
                    let body = render_expr(&value_expr(&cell.program, &names, &[], &inputs)?);
                    source.push_str(&body);
                    source.push_str(" }");
                    drop(_reference);
                    if rate_context() || fold_context() {
                        return Ok(map_runtime_result(format!("(|| -> Result<_, String> {{ Ok({source}) }})()")));
                    }
                    return Ok(Expr::Raw(source));
                }
            }
            let binding = emath_exec_ir::native_kernel::verified_kernel_binding(capability)
                .map_err(|error| match error {
                    emath_exec_ir::native_kernel::KernelBindingError::MissingBinding(_) => {
                        BackendError::MissingArtifactBinding(capability.to_string())
                    }
                    _ => BackendError::StaleArtifactBinding(capability.to_string()),
                })?;
            if let Some(artifact) = emath_exec_ir::native_kernel::checked::verified(capability) {
                let inputs = binding
                    .signature
                    .strip_prefix('(')
                    .and_then(|s| s.split_once(")->"))
                    .map(|(s, _)| s)
                    .unwrap_or("");
                let types: Vec<_> = inputs.split(',').collect();
                let arguments = args
                    .iter()
                    .enumerate()
                    .map(|(index, arg)| {
                        let value = render_expr(&operand(program, *arg));
                        if artifact.borrowed & (1 << index) != 0 {
                            format!("&{value}")
                        } else if types.get(index) == Some(&"Float64") {
                            format!("({value}) as f64")
                        } else {
                            value
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                return Ok(map_runtime_result(format!(
                    "emath_rt::{}({arguments})", artifact.rust_function
                )));
            }
            let artifact = KERNEL_ARTIFACTS
                .iter()
                .find(|artifact| {
                    artifact.kernel_id == binding.kernel_id
                        && artifact.signature == binding.signature
                        && artifact.semantic_hash == binding.semantic_hash
                })
                .ok_or_else(|| BackendError::StaleArtifactBinding(capability.to_string()))?;
            artifact
                .render(args, program, kinds)
                .ok_or_else(|| BackendError::UnsupportedType(format!(
                    "kernel {} refuses argument carriers {:?}",
                    artifact.kernel_id,
                    args.iter().map(|arg| kind_at(kinds, *arg)).collect::<Vec<_>>()
                )))
        }
        _ => Err(BackendError::MissingArtifactContract(
            capability.to_string(),
        )),
    }
}

struct KernelArtifact {
    kernel_id: &'static str,
    signature: &'static str,
    semantic_hash: &'static str,
    kind: KernelArtifactKind,
}

enum KernelArtifactKind {
    Sampling(u8),
    DensePointIndex,
}

impl KernelArtifact {
    fn render(&self, args: &[EmirValue], program: &EmirProgram, kinds: &[ValueKind]) -> Option<Expr> {
        let kind = match self.kind {
            KernelArtifactKind::DensePointIndex => return op_collections::dense_point_index_expr(args, program, kinds),
            KernelArtifactKind::Sampling(kind) => kind,
        };
        let [params, seed, draws, tail @ ..] = args else {
            return None;
        };
        let stream = match tail {
            [] => Expr::Str(String::new()),
            [stream] => Expr::Raw(format!("&{}", render_expr(&operand_ref(program, *stream)))),
            _ => return None,
        };
        Some(Expr::Call {
            path: vec![
                "emath_rt".to_string(),
                "probability".to_string(),
                "prob_sample_in_stream".to_string(),
            ],
            args: vec![
                Expr::Raw(kind.to_string()),
                operand_ref(program, *params),
                operand_ref(program, *seed),
                operand_ref(program, *draws),
                stream,
            ],
        })
    }
}

const SAMPLING_SIGNATURE: &str = "(Vector<Float64>,Float64,Float64,Text?)->Vector<Float64>";

const KERNEL_ARTIFACTS: &[KernelArtifact] = &[
    KernelArtifact {
        kernel_id: "checked-dense-index",
        signature: "(Dense<Float64>,Sequence)->Float64",
        semantic_hash: "sha256:c278f35784be8439c9141efaa734b85ce1cb9df8b479e8bc1b8dbe27561a0c8a",
        kind: KernelArtifactKind::DensePointIndex,
    },
    KernelArtifact {
        kernel_id: "counter-stream-gaussian-transform",
        signature: SAMPLING_SIGNATURE,
        semantic_hash: "sha256:aea62740b00c48e611f84b99fde824e01457ccb1e79ee4de8a218182577a145e",
        kind: KernelArtifactKind::Sampling(0),
    },
    KernelArtifact {
        kernel_id: "counter-stream-affine-transform",
        signature: SAMPLING_SIGNATURE,
        semantic_hash: "sha256:e57bc8668a6a85953899f2ff59add385e964d88e7ee00bf1d1ca7da5a798644c",
        kind: KernelArtifactKind::Sampling(1),
    },
    KernelArtifact {
        kernel_id: "counter-stream-threshold-transform",
        signature: SAMPLING_SIGNATURE,
        semantic_hash: "sha256:9b3fe206334c592da948e6ccb4b15baa75aec7df35b43795125820b28eafbf23",
        kind: KernelArtifactKind::Sampling(2),
    },
];

fn literal_frame_expr(body: &EmirProgram, inputs: &[EmirValue], state: &[EmirValue], outer: &EmirProgram, kinds: &[ValueKind]) -> Result<Expr, BackendError> {
    if inputs.len() != usize::from(body.input_count) || state.len() != usize::from(body.state_count) {
        return Err(BackendError::UnsupportedType("frame argument count mismatch".into()));
    }
    let names = (0..inputs.len()).map(|index| format!("__frame_input_{index}")).collect::<Vec<_>>();
    let states = (0..state.len()).map(|index| format!("__frame_state_{index}")).collect::<Vec<_>>();
    let mut frame_kinds = InputKinds::new();
    let mut code = String::from("{ ");
    for (name, value) in names.iter().zip(inputs).chain(states.iter().zip(state)) {
        let kind = kind_at(kinds, *value);
        let borrow = if kind.is_copy() { "" } else { "&" };
        code.push_str(&format!("let {name} = {borrow}{}; ", render_expr(&operand(outer, *value))));
        frame_kinds.insert(name.clone(), kind);
    }
    let _reference = ReferenceScope::enter();
    let _state = LocalStateScope::enter();
    code.push_str(&render_expr(&value_expr(body, &names, &states, &frame_kinds)?));
    code.push_str(" }");
    Ok(Expr::Raw(code))
}
