//! Semantics-preserving optimization boundary for universal bytecode.
//!
//! Capability applications and storage/control instructions are deliberately
//! opaque. Kernel selection and feature semantics are not optimizer authority.

use crate::{EmirOp, EmirProgram, EmirValue};

/// Optimize a program without looking through capability boundaries.
///
/// The final universal machine currently performs no speculative rewrites;
/// authored reference bytecode and kernel bindings remain the authority.
pub fn optimize_program(_program: &mut EmirProgram) {}

/// Enumerate register operands for generic liveness consumers.
pub fn operand_registers(op: &EmirOp, out: &mut Vec<EmirValue>) {
    match op {
        EmirOp::ConstF64(_)
        | EmirOp::ConstI64(_)
        | EmirOp::ConstBigInt(_)
        | EmirOp::ConstText(_)
        | EmirOp::ConstComplex(_, _)
        | EmirOp::ConstBool(_)
        | EmirOp::LoadInput(_)
        | EmirOp::LoadState(_)
        | EmirOp::SeriesCreate { .. }
        | EmirOp::OptionNone => {}
        EmirOp::F64Add(a, b)
        | EmirOp::F64Sub(a, b)
        | EmirOp::F64Mul(a, b)
        | EmirOp::F64Div(a, b)
        | EmirOp::F64Pow(a, b)
        | EmirOp::Lt(a, b)
        | EmirOp::Le(a, b)
        | EmirOp::Gt(a, b)
        | EmirOp::Ge(a, b)
        | EmirOp::Eq(a, b)
        | EmirOp::Ne(a, b)
        | EmirOp::And(a, b)
        | EmirOp::Or(a, b)
        | EmirOp::Imply(a, b)
        | EmirOp::Iff(a, b)
        | EmirOp::BinaryBuiltin(_, a, b)
        | EmirOp::OptionUnwrapOr(a, b)
        | EmirOp::ResultUnwrapOr(a, b) => out.extend([*a, *b]),
        EmirOp::Neg(value)
        | EmirOp::UnaryBuiltin(_, value)
        | EmirOp::Not(value)
        | EmirOp::IsFinite(value)
        | EmirOp::OptionSome(value)
        | EmirOp::OptionIsSome(value)
        | EmirOp::ResultOk(value)
        | EmirOp::ResultErr(value)
        | EmirOp::ResultIsOk(value)
        | EmirOp::ResultErrorOf(value)
        | EmirOp::VectorAllFinite(value) => out.push(*value),
        EmirOp::Select {
            condition,
            then_value,
            else_value,
        } => {
            out.extend([*condition, *then_value, *else_value]);
        }
        EmirOp::FormatText { arguments, .. }
        | EmirOp::VectorCreate(arguments)
        | EmirOp::ApplyCapability {
            args: arguments, ..
        } => out.extend(arguments.iter().copied()),
        EmirOp::SeriesSample { series, time } => out.extend([*series, *time]),
        EmirOp::SetCreate { elements, guards } => {
            out.extend(elements.iter().copied());
            out.extend(guards.iter().flatten().copied());
        }
        EmirOp::SetContains { element, set } => out.extend([*element, *set]),
        EmirOp::RecordCreate { fields, .. } => out.extend(fields.iter().map(|(_, value)| *value)),
        EmirOp::MatrixCreate { elements, .. } | EmirOp::TensorCreate { elements, .. } => {
            out.extend(elements.iter().copied());
        }
        EmirOp::VectorIndex { vector, index } => out.extend([*vector, *index]),
        EmirOp::MatrixIndex { matrix, row, col } => out.extend([*matrix, *row, *col]),
        EmirOp::TensorIndex { tensor, indices } => {
            out.push(*tensor);
            out.extend(indices.iter().copied());
        }
        EmirOp::TensorSlice { tensor, axes } => {
            out.push(*tensor);
            for axis in axes {
                match axis {
                    crate::EmirSliceAxis::Point(value) => out.push(*value),
                    crate::EmirSliceAxis::Range { start, end } => out.extend([*start, *end]),
                }
            }
        }
        EmirOp::Fold {
            start, end, init, ..
        } => out.extend([*start, *end, *init]),
        EmirOp::VectorMap { source, .. } | EmirOp::VectorReduce { source, .. } => out.push(*source),
        EmirOp::VectorMapScalar { vector, scalar, .. } => out.extend([*vector, *scalar]),
        // The literal carries its own nested register namespace; it
        // consumes no outer registers.
        EmirOp::ProgramLiteral(_) => {}
    }
}

/// Only immutable constants and direct loads are unconditionally total.
pub fn is_total(op: &EmirOp, _program: &EmirProgram) -> bool {
    matches!(
        op,
        EmirOp::ConstF64(_)
            | EmirOp::ConstI64(_)
            | EmirOp::ConstBigInt(_)
            | EmirOp::ConstText(_)
            | EmirOp::ConstComplex(_, _)
            | EmirOp::ConstBool(_)
            | EmirOp::LoadInput(_)
            | EmirOp::LoadState(_)
    )
}
