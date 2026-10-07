use super::{EmirProgram, EmirOp, Value, ValueFrame, EvalBudget, EvalFault, eval_fold, apply_capability, register};

pub(super) fn eval_aggregate(
    _self_program: &EmirProgram,
    op: &EmirOp,
    registers: &[Value],
    inputs: ValueFrame<'_>,
    state: ValueFrame<'_>,
    budget: EvalBudget,
) -> Result<Value, EvalFault> {
    match op {
        EmirOp::Fold {
            start,
            end,
            init,
            combine,
            loop_var_index,
            body,
        } => eval_fold(
            registers,
            inputs,
            state,
            *start,
            *end,
            *init,
            *combine,
            *loop_var_index,
            body,
        ),
        EmirOp::ApplyCapability {
            capability,
            class,
            args,
        } => apply_capability(capability, *class, args, registers, budget),
        EmirOp::ToF64Vector(sequence) => match register(registers, *sequence)? {
            value @ Value::Vector(_) => Ok(value.clone()),
            Value::List(values) => values
                .iter()
                .map(|value| match value {
                    Value::F64(value) => Ok(*value),
                    _ => Err(EvalFault::TypeConfusion {
                        register: sequence.0,
                        op: "to-f64-vector",
                    }),
                })
                .collect::<Result<Vec<_>, _>>()
                .map(Value::Vector),
            _ => Err(EvalFault::TypeConfusion {
                register: sequence.0,
                op: "to-f64-vector",
            }),
        },
        _ => unreachable!("op routed to the wrong eval family"),
    }
}
