use super::*;

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
        EmirOp::VectorMap { builtin, source } => {
            let values = vector_of(registers, *source, "vector-map")?
                .iter()
                .map(|value| builtin.eval_unary(*value))
                .collect::<Option<Vec<_>>>()
                .ok_or(EvalFault::Arithmetic {
                    op: "vector-map",
                    detail: "binary scalar opcode used in map instruction",
                })?;
            Ok(Value::Vector(values))
        }
        EmirOp::VectorMapScalar { op, vector, scalar } => {
            let scalar = f64_of(registers, *scalar, "vector-map-scalar")?;
            let values = vector_of(registers, *vector, "vector-map-scalar")?;
            Ok(Value::Vector(
                values
                    .iter()
                    .map(|value| match op {
                        VectorScalarOp::Add => *value + scalar,
                        VectorScalarOp::Sub => *value - scalar,
                        VectorScalarOp::Mul => *value * scalar,
                        VectorScalarOp::Div => *value / scalar,
                    })
                    .collect(),
            ))
        }
        EmirOp::VectorReduce { reduce, source } => {
            let values = vector_of(registers, *source, "vector-reduce")?;
            let Some(first) = values.first().copied() else {
                return Err(EvalFault::Arithmetic {
                    op: "vector-reduce",
                    detail: "empty vector",
                });
            };
            let value = match reduce {
                ReduceId::Sum => values.iter().sum(),
                ReduceId::Max => values.iter().copied().fold(first, f64::max),
                ReduceId::Min => values.iter().copied().fold(first, f64::min),
            };
            Ok(Value::F64(value))
        }
        EmirOp::VectorAllFinite(source) => Ok(Value::Bool(
            vector_of(registers, *source, "vector-all-finite")?
                .iter()
                .all(|value| value.is_finite()),
        )),
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
