use super::{EmirProgram, EmirOp, Value, ValueFrame, EvalBudget, EvalFault, eval_control, eval_const, eval_scalar_arith, eval_structure, eval_math, eval_option_result, eval_aggregate, eval_calls};

pub(super) fn eval_op(
    self_program: &EmirProgram,
    op: &EmirOp,
    registers: &[Value],
    inputs: ValueFrame<'_>,
    state: ValueFrame<'_>,
    budget: EvalBudget,
) -> Result<Value, EvalFault> {
    match op {
        // Family router: OR-patterns are exhaustive (compiler-enforced),
        // so a new EmirOp variant must be placed in exactly one family.
        EmirOp::ListCreate(_) | EmirOp::RecordField { .. } | EmirOp::Refuse(_) | EmirOp::Branch { .. } | EmirOp::Collect { .. } | EmirOp::Iterate { .. } => eval_control(
            self_program, op, registers, inputs, state, budget,
        ),
        EmirOp::ConstF64(_) | EmirOp::ConstI64(_) | EmirOp::ConstExactInt(_) | EmirOp::ConstText(_) | EmirOp::ConstBool(_) | EmirOp::LoadInput(_) => eval_const(
            self_program, op, registers, inputs, state, budget,
        ),
        EmirOp::F64Add(_, _) | EmirOp::F64Sub(_, _) | EmirOp::F64Mul(_, _) | EmirOp::F64Div(_, _) | EmirOp::Neg(_) | EmirOp::ExactIntCall { .. } | EmirOp::SameBits(_, _) | EmirOp::Lt(_, _) | EmirOp::Le(_, _) | EmirOp::Gt(_, _) | EmirOp::Ge(_, _) | EmirOp::Eq(_, _) | EmirOp::Ne(_, _) | EmirOp::And(_, _) | EmirOp::Or(_, _) | EmirOp::Not(_) | EmirOp::IsFinite(_) => eval_scalar_arith(
            self_program, op, registers, inputs, state, budget,
        ),
        EmirOp::FormatText { .. } | EmirOp::RecordCreate { .. } | EmirOp::VectorLength(_) | EmirOp::IndexText(_) | EmirOp::RefuseValue(_) | EmirOp::ListConcat(_) => eval_structure(
            self_program, op, registers, inputs, state, budget,
        ),
        EmirOp::TextTrim(_) | EmirOp::TextLength(_) | EmirOp::ParseF64(_) | EmirOp::TextByte(_, _) | EmirOp::FormatScientific(_, _) | EmirOp::VectorIndex { .. } => eval_math(
            self_program, op, registers, inputs, state, budget,
        ),
        EmirOp::ResultIsOk(_) | EmirOp::ResultUnwrapOr(_, _) | EmirOp::TensorSlice { .. } => eval_option_result(
            self_program, op, registers, inputs, state, budget,
        ),
        EmirOp::Fold { .. } | EmirOp::ApplyCapability { .. } | EmirOp::ToF64Vector(_) => eval_aggregate(
            self_program, op, registers, inputs, state, budget,
        ),
        EmirOp::CallFrame { .. } | EmirOp::CallSelf { .. } | EmirOp::F64SortTotal(_) | EmirOp::ProgramLiteral { .. } | EmirOp::CallProgram { .. }
            | EmirOp::CallScalarProgram { .. } | EmirOp::CallRealProgram { .. }
            | EmirOp::TryCallRealProgram { .. } | EmirOp::CallValue { .. } => eval_calls(
            self_program, op, registers, inputs, state, budget,
        ),
        // Emission-carried quote ops: the constructor lane computes
        // quotes through its own Code machinery (the reference VM's
        // CValue::Code); the E-MIR interpreter refuses them by name
        // instead of guessing a carrier.
        EmirOp::CodeLiteral { .. }
        | EmirOp::CodeSubstitute { .. }
        | EmirOp::CodeEvaluate { .. }
        | EmirOp::CodeView { .. }
        | EmirOp::CodeMake { .. }
        | EmirOp::CodeBody { .. }
        | EmirOp::CodeOpen { .. }
        | EmirOp::CodeBind { .. }
        | EmirOp::CodeIdentity { .. }
        | EmirOp::CodeJoin { .. } => {
            Err(EvalFault::CarrierRefused {
                op: op.name(),
                detail: String::from(
                    "emission-carried quote op; the constructor lane computes quotes through its own Code machinery",
                ),
            })
        }
        // The engine call names a module function; the constructor VM
        // runs those directly, so this interpreter never needs one.
        EmirOp::BufferCreate { .. } | EmirOp::BufferWrite { .. } | EmirOp::EngineCall { .. } => Err(EvalFault::CarrierRefused {
            op: op.name(),
            detail: String::from(
                "constructor-carried operation; constructor-lowered programs run in the constructor VM",
            ),
        }),
        // A sibling entry call is emission-lane: the interpreter has
        // no sibling-name environment, and constructor-lowered
        // programs run in the constructor VM, which resolves sibling
        // names natively. Refuse by name instead of guessing.
        EmirOp::CallSibling { .. } => Err(EvalFault::CarrierRefused {
            op: op.name(),
            detail: String::from(
                "sibling entry call; the interpreter has no sibling-name environment",
            ),
        }),
    }
}
