use super::*;

pub(super) fn eval_control(
    self_program: &EmirProgram,
    op: &EmirOp,
    registers: &[Value],
    _inputs: ValueFrame<'_>,
    _state: ValueFrame<'_>,
    budget: EvalBudget,
) -> Result<Value, EvalFault> {
    match op {
        EmirOp::ListCreate(elements) => elements
            .iter()
            .map(|value| register(registers, *value).cloned())
            .collect::<Result<Vec<_>, _>>()
            .map(Value::List),
        EmirOp::RecordField { record, field } => {
            let Value::Record { fields, .. } = register(registers, *record)? else {
                return Err(EvalFault::TypeConfusion {
                    register: record.0,
                    op: "record-field",
                });
            };
            fields
                .get(field)
                .cloned()
                .ok_or_else(|| EvalFault::CarrierRefused {
                    op: "record-field",
                    detail: format!("record has no field `{field}`"),
                })
        }
        EmirOp::Refuse(detail) => Err(EvalFault::CarrierRefused {
            op: "refuse",
            detail: detail.clone(),
        }),
        EmirOp::Branch {
            condition,
            args,
            then_body,
            else_body,
        } => {
            let selected = bool_of(registers, *condition, "branch")?;
            let _site = crate::progress::site(emath_core::Span::default(), usize::from(selected));
            let body = if selected { then_body } else { else_body };
            let arguments = args
                .iter()
                .map(|arg| register(registers, *arg).cloned())
                .collect::<Result<Vec<_>, _>>()?;
            evaluate_frames_in(
                self_program,
                body,
                ValueFrame::direct(&arguments),
                ValueFrame::direct(&[]),
                budget,
            )
        }
        EmirOp::Collect { count, args, body } => {
            let count = i64_of(registers, *count, "collect")?;
            let count = usize::try_from(count).map_err(|_| EvalFault::CarrierRefused {
                op: "collect",
                detail: "collection count must be nonnegative and fit usize".into(),
            })?;
            if count > budget.max_steps as usize {
                return Err(EvalFault::BudgetExhausted { executed: 0 });
            }
            let mut values = Vec::new();
            values
                .try_reserve_exact(count)
                .map_err(|_| EvalFault::CarrierRefused {
                    op: "collect",
                    detail: "collection allocation exceeds available capacity".into(),
                })?;
            let mut arguments = Vec::with_capacity(args.len() + 1);
            arguments.push(Value::I64(0));
            for value in args {
                arguments.push(register(registers, *value)?.clone());
            }
            for index in 0..count {
                let _site = crate::progress::site(emath_core::Span::default(), index);
                EvaluationFuel::charge(false)?;
                arguments[0] = Value::I64(index as i64);
                values.push(evaluate_with_budget(body, &arguments, &[], budget)?);
            }
            Ok(Value::List(values))
        }
        EmirOp::Iterate {
            count,
            init,
            args,
            stop,
            body,
        } => {
            let Value::I64(count_value) = register(registers, *count)? else {
                return Err(EvalFault::TypeConfusion {
                    register: count.0,
                    op: "iterate",
                });
            };
            if *count_value < 0 {
                return Err(EvalFault::Arithmetic {
                    op: "iterate",
                    detail: "iteration count must be nonnegative",
                });
            }
            let mut arguments = Vec::with_capacity(args.len() + 2);
            arguments.push(Value::I64(0));
            arguments.push(register(registers, *init)?.clone());
            for arg in args {
                arguments.push(register(registers, *arg)?.clone());
            }
            for index in 0..*count_value {
                let _site = crate::progress::site(emath_core::Span::default(), index as usize);
                arguments[0] = Value::I64(index);
                if let Some(stop) = stop {
                    let done = evaluate_with_budget(stop, &arguments, &[], budget)?;
                    if value_as_bool(&done, "iterate-stop")? {
                        break;
                    }
                }
                // Charge even an empty body so iteration cannot bypass fuel.
                EvaluationFuel::charge(false)?;
                arguments[1] = evaluate_with_budget(body, &arguments, &[], budget)?;
            }
            Ok(arguments.swap_remove(1))
        }
        _ => unreachable!("op routed to the wrong eval family"),
    }
}
