//! Semantics-preserving optimization boundary for universal bytecode.
//!
//! Capability applications and storage/control instructions are deliberately
//! opaque. Kernel selection and feature semantics are not optimizer authority.

use crate::{CodeSubRef, EmirOp, EmirProgram, EmirValue};

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
        | EmirOp::ConstExactInt(_)
        | EmirOp::ConstText(_)
        | EmirOp::ConstBool(_)
        | EmirOp::LoadInput(_)
        | EmirOp::Refuse(_) => {}
        EmirOp::F64Add(a, b)
        | EmirOp::F64Sub(a, b)
        | EmirOp::F64Mul(a, b)
        | EmirOp::F64Div(a, b)
        | EmirOp::Lt(a, b)
        | EmirOp::Le(a, b)
        | EmirOp::Gt(a, b)
        | EmirOp::Ge(a, b)
        | EmirOp::SameBits(a, b)
        | EmirOp::TextByte(a, b)
        | EmirOp::FormatScientific(a, b)
        | EmirOp::Eq(a, b)
        | EmirOp::Ne(a, b)
        | EmirOp::And(a, b)
        | EmirOp::Or(a, b)
        | EmirOp::ResultUnwrapOr(a, b) => out.extend([*a, *b]),
        EmirOp::Neg(value)
        | EmirOp::Not(value)
        | EmirOp::IsFinite(value)
        | EmirOp::VectorLength(value)
        | EmirOp::ToF64Vector(value)
        | EmirOp::TextTrim(value)
        | EmirOp::TextLength(value)
        | EmirOp::IndexText(value)
        | EmirOp::RefuseValue(value)
        | EmirOp::ParseF64(value)
        | EmirOp::ResultIsOk(value) => out.push(*value),
        EmirOp::FormatText { arguments, .. }
        | EmirOp::ListCreate(arguments)
        | EmirOp::ApplyCapability {
            args: arguments, ..
        }
        | EmirOp::ExactIntCall {
            args: arguments, ..
        } => out.extend(arguments.iter().copied()),
        EmirOp::CallProgram { program, inputs }
        | EmirOp::CallScalarProgram { program, inputs }
        | EmirOp::CallRealProgram { program, inputs }
        | EmirOp::TryCallRealProgram { program, inputs } => out.extend([*program, *inputs]),
        EmirOp::CallValue { program, inputs } => {
            out.push(*program);
            out.extend(inputs.iter().copied());
        }
        EmirOp::RecordCreate { fields, .. } => out.extend(fields.iter().map(|(_, value)| *value)),
        EmirOp::RecordField { record, .. } => out.push(*record),
        EmirOp::Branch { condition, args, .. } => { out.push(*condition); out.extend(args.iter().copied()); }
        EmirOp::Collect { count, args, .. } => { out.push(*count); out.extend(args.iter().copied()); }
        EmirOp::Iterate { count, init, args, .. } => { out.extend([*count, *init]); out.extend(args.iter().copied()); }
        EmirOp::VectorIndex { vector, index } => out.extend([*vector, *index]),
        EmirOp::BufferCreate { size, fill } => out.extend([*size, *fill]),
        EmirOp::BufferWrite { buffer, index, value } => out.extend([*buffer, *index, *value]),
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
        EmirOp::CallFrame { inputs, state, .. } => { out.extend(inputs); out.extend(state); }
        EmirOp::CallSelf { inputs, .. } => out.extend(inputs),
        EmirOp::CallSibling { inputs, .. } => out.extend(inputs),
        EmirOp::F64SortTotal(value) => out.push(*value),
        EmirOp::ListConcat(values) => out.extend(values),
        // Body registers are local; capture registers belong to this frame.
        EmirOp::ProgramLiteral { captures, .. } => out.extend(captures),
        // Quote-code ops: the code/value registers belong to this
        // frame; a literal's nested body registers stay local (the
        // ProgramLiteral rule).
        EmirOp::CodeLiteral { .. } => {}
        EmirOp::CodeSubstitute { code, reference, value } => {
            out.extend([*code, *value]);
            if let CodeSubRef::Dyn(reference) = reference {
                out.push(*reference);
            }
        }
        EmirOp::CodeEvaluate { code, .. } => out.push(*code),
        EmirOp::CodeView { code } => out.push(*code),
        EmirOp::CodeMake { node } => out.push(*node),
        EmirOp::CodeBody { code } => out.push(*code),
        EmirOp::CodeOpen { package } => out.push(*package),
        EmirOp::CodeBind { code } | EmirOp::CodeIdentity { code } => out.push(*code),
        EmirOp::CodeJoin { left, right } => out.extend([*left, *right]),
        EmirOp::EngineCall { inputs, .. } => out.extend(inputs.iter().map(|(_, value)| *value)),
    }
}

/// Only immutable constants and direct loads are unconditionally total.
pub fn is_total(op: &EmirOp, _program: &EmirProgram) -> bool {
    matches!(
        op,
        EmirOp::ConstF64(_)
            | EmirOp::ConstI64(_)
            | EmirOp::ConstExactInt(_)
            | EmirOp::ConstText(_)
            | EmirOp::ConstBool(_)
            | EmirOp::LoadInput(_)
    )
}
