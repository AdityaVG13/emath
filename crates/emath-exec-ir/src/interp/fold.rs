use super::*;

pub(super) fn checked_index(
    registers: &[Value],
    value: EmirValue,
    len: usize,
    op: &'static str,
) -> Result<usize, EvalFault> {
    let raw = i64_of(registers, value, op)?;
    usize::try_from(raw)
        .ok()
        .filter(|index| *index < len)
        .ok_or(EvalFault::IndexOutOfBounds {
            op,
            index: raw,
            len,
        })
}

pub(super) fn eval_fold(
    registers: &[Value],
    inputs: ValueFrame<'_>,
    state: ValueFrame<'_>,
    start: EmirValue,
    end: EmirValue,
    init: EmirValue,
    combine: FoldCombine,
    loop_var_index: u16,
    body: &EmirProgram,
) -> Result<Value, EvalFault> {
    let start = i64_of(registers, start, "fold")?;
    let end = i64_of(registers, end, "fold")?;
    let mut accumulator = register(registers, init)?.clone();
    let mut body_inputs = inputs.to_vec();
    let slot = usize::from(loop_var_index);
    if body_inputs.len() <= slot {
        body_inputs.resize(slot + 1, Value::I64(0));
    }
    for (iteration, value) in (start..end).enumerate() {
        let _site = crate::progress::site(emath_core::Span::default(), iteration);
        body_inputs[slot] = Value::I64(value);
        let next = evaluate_frames(
            body,
            ValueFrame::direct(&body_inputs),
            state,
            EvalBudget::default(),
        )?;
        accumulator = match combine {
            FoldCombine::Add => {
                Value::F64(value_as_f64(&accumulator, "fold")? + value_as_f64(&next, "fold")?)
            }
            FoldCombine::Mul => {
                Value::F64(value_as_f64(&accumulator, "fold")? * value_as_f64(&next, "fold")?)
            }
            FoldCombine::And => {
                Value::Bool(value_as_bool(&accumulator, "fold")? && value_as_bool(&next, "fold")?)
            }
            FoldCombine::Or => {
                Value::Bool(value_as_bool(&accumulator, "fold")? || value_as_bool(&next, "fold")?)
            }
        };
    }
    Ok(accumulator)
}

