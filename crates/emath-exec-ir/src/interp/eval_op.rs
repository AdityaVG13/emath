use super::*;

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
        EmirOp::ConstF64(_) | EmirOp::ConstI64(_) | EmirOp::ConstExactInt(_) | EmirOp::ConstBigInt(_) | EmirOp::ConstText(_) | EmirOp::ConstComplex(_, _) | EmirOp::ConstBool(_) | EmirOp::LoadInput(_) | EmirOp::LoadState(_) => eval_const(
            self_program, op, registers, inputs, state, budget,
        ),
        EmirOp::F64Add(_, _) | EmirOp::F64Sub(_, _) | EmirOp::F64Mul(_, _) | EmirOp::F64Div(_, _) | EmirOp::F64Pow(_, _) | EmirOp::Neg(_) | EmirOp::ToInt(_) | EmirOp::IntegerQuotient(_, _) | EmirOp::ExactIntCall { .. } | EmirOp::SameBits(_, _) | EmirOp::UnaryBuiltin(_, _) | EmirOp::BinaryBuiltin(_, _, _) | EmirOp::Lt(_, _) | EmirOp::Le(_, _) | EmirOp::Gt(_, _) | EmirOp::Ge(_, _) | EmirOp::Eq(_, _) | EmirOp::Ne(_, _) | EmirOp::And(_, _) | EmirOp::Or(_, _) | EmirOp::Imply(_, _) | EmirOp::Iff(_, _) | EmirOp::Not(_) | EmirOp::IsFinite(_) => eval_scalar_arith(
            self_program, op, registers, inputs, state, budget,
        ),
        EmirOp::Select { .. } | EmirOp::FormatText { .. } | EmirOp::SeriesCreate { .. } | EmirOp::SeriesSample { .. } | EmirOp::SetCreate { .. } | EmirOp::SetContains { .. } | EmirOp::RecordCreate { .. } | EmirOp::VectorLength(_) | EmirOp::VectorCreate(_) | EmirOp::MatrixCreate { .. } | EmirOp::MatrixRows(_) | EmirOp::MatrixCols(_) | EmirOp::MatrixPack { .. } | EmirOp::IndexText(_) | EmirOp::RefuseValue(_) => eval_structure(
            self_program, op, registers, inputs, state, budget,
        ),
        EmirOp::F64Exp2(_) | EmirOp::F64PowI(_, _) | EmirOp::TextTrim(_) | EmirOp::TextLength(_) | EmirOp::ParseF64(_) | EmirOp::TextByte(_, _) | EmirOp::FormatScientific(_, _) | EmirOp::TensorShape(_) | EmirOp::TensorPack { .. } | EmirOp::DenseIndex { .. } | EmirOp::TensorCreate { .. } | EmirOp::VectorIndex { .. } | EmirOp::MatrixIndex { .. } | EmirOp::TensorIndex { .. } => eval_math(
            self_program, op, registers, inputs, state, budget,
        ),
        EmirOp::OptionSome(_) | EmirOp::OptionNone | EmirOp::OptionIsSome(_) | EmirOp::OptionUnwrapOr(_, _) | EmirOp::ResultOk(_) | EmirOp::ResultErr(_) | EmirOp::ResultIsOk(_) | EmirOp::ResultUnwrapOr(_, _) | EmirOp::ResultErrorOf(_) | EmirOp::TensorSlice { .. } => eval_option_result(
            self_program, op, registers, inputs, state, budget,
        ),
        EmirOp::Fold { .. } | EmirOp::ApplyCapability { .. } | EmirOp::VectorMap { .. } | EmirOp::VectorMapScalar { .. } | EmirOp::VectorReduce { .. } | EmirOp::VectorAllFinite(_) | EmirOp::ToF64Vector(_) => eval_aggregate(
            self_program, op, registers, inputs, state, budget,
        ),
        EmirOp::CallFrame { .. } | EmirOp::CallSelf { .. } | EmirOp::DenseLayout(_) | EmirOp::VectorSlice { .. } | EmirOp::F64SortTotal(_) | EmirOp::VectorConcat(_) | EmirOp::SameDenseShape(_, _) | EmirOp::ToF64(_) | EmirOp::DenseValues(_) | EmirOp::DenseRepack { .. } | EmirOp::ProgramLiteral { .. } | EmirOp::CallProgram { .. }
            | EmirOp::CallScalarProgram { .. } | EmirOp::CallRealProgram { .. }
            | EmirOp::TryCallRealProgram { .. } => eval_calls(
            self_program, op, registers, inputs, state, budget,
        ),
    }
}
