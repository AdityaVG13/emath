use std::collections::{BTreeSet, HashMap};

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
use op_flow::op_flow_exprs;

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
    i64_names: &BTreeSet<String>,
) -> Result<Expr, BackendError> {
    match op {
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
        | EmirOp::RecordCreate { .. } => op_data_exprs(op, program, names, states),
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
            &scalar_kinds(program, names, states, i64_names),
        ),
        EmirOp::Select { .. }
        | EmirOp::VectorCreate(..)
        | EmirOp::MatrixCreate { .. }
        | EmirOp::TensorCreate { .. }
        | EmirOp::VectorIndex { .. }
        | EmirOp::MatrixIndex { .. }
        | EmirOp::TensorIndex { .. }
        | EmirOp::TensorSlice { .. } => op_collection_exprs(
            op,
            program,
            &scalar_kinds(program, names, states, i64_names),
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
            i64_names,
            &scalar_kinds(program, names, states, i64_names),
        ),
        EmirOp::ApplyCapability {
            capability,
            class,
            args,
        } => capability_artifact_expr(capability, *class, args, program),
        EmirOp::ProgramLiteral(_)
        | EmirOp::VectorMap { .. }
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
            let binding = emath_exec_ir::native_kernel::verified_kernel_binding(capability)
                .map_err(|error| match error {
                    emath_exec_ir::native_kernel::KernelBindingError::MissingBinding(_) => {
                        BackendError::MissingArtifactBinding(capability.to_string())
                    }
                    _ => BackendError::StaleArtifactBinding(capability.to_string()),
                })?;
            let artifact = KERNEL_ARTIFACTS
                .iter()
                .find(|artifact| {
                    artifact.kernel_id == binding.kernel_id
                        && artifact.signature == binding.signature
                        && artifact.semantic_hash == binding.semantic_hash
                })
                .ok_or_else(|| BackendError::StaleArtifactBinding(capability.to_string()))?;
            artifact.render(args, program).ok_or_else(|| {
                BackendError::StaleArtifactBinding(capability.to_string())
            })
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
    family: &'static str,
}

impl KernelArtifact {
    fn render(&self, args: &[EmirValue], program: &EmirProgram) -> Option<Expr> {
        let [params, seed, draws, tail @ ..] = args else {
            return None;
        };
        let stream = match tail {
            [] => Expr::Str(String::new()),
            [stream] => Expr::Raw(format!(
                "&{}",
                render_expr(&operand_ref(program, *stream))
            )),
            _ => return None,
        };
        Some(Expr::Call {
            path: vec![
                "emath_rt".to_string(),
                "probability".to_string(),
                "prob_sample_in_stream".to_string(),
            ],
            args: vec![
                Expr::Raw(format!(
                    "emath_rt::probability::Family::{}",
                    self.family
                )),
                operand_ref(program, *params),
                operand_ref(program, *seed),
                operand_ref(program, *draws),
                stream,
            ],
        })
    }
}

const SAMPLING_SIGNATURE: &str =
    "(Vector<Float64>,Float64,Float64,Text?)->Vector<Float64>";

const KERNEL_ARTIFACTS: &[KernelArtifact] = &[
    KernelArtifact {
        kernel_id: "counter-stream-gaussian-transform",
        signature: SAMPLING_SIGNATURE,
        semantic_hash: "sha256:aea62740b00c48e611f84b99fde824e01457ccb1e79ee4de8a218182577a145e",
        family: "Normal",
    },
    KernelArtifact {
        kernel_id: "counter-stream-affine-transform",
        signature: SAMPLING_SIGNATURE,
        semantic_hash: "sha256:e57bc8668a6a85953899f2ff59add385e964d88e7ee00bf1d1ca7da5a798644c",
        family: "Uniform",
    },
    KernelArtifact {
        kernel_id: "counter-stream-threshold-transform",
        signature: SAMPLING_SIGNATURE,
        semantic_hash: "sha256:9b3fe206334c592da948e6ccb4b15baa75aec7df35b43795125820b28eafbf23",
        family: "Bernoulli",
    },
];
