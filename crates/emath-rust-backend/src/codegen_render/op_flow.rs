//! Universal fold and native Rust carrier lowering.

use super::*;

pub(super) fn op_flow_exprs(
    op: &EmirOp,
    program: &EmirProgram,
    names: &[String],
    states: &[String],
    i64_names: &BTreeSet<String>,
    kinds: &[ScalarKind],
) -> Result<Expr, BackendError> {
    match op {
        EmirOp::Fold {
            start,
            end,
            init,
            combine,
            loop_var_index,
            body,
        } => {
            let mut body_names = names.to_vec();
            let slot = *loop_var_index as usize;
            while body_names.len() <= slot {
                body_names.push(String::new());
            }
            let loop_name = format!("__loop{slot}");
            body_names[slot] = loop_name.clone();
            let i64_fold = fold_is_i64(
                kinds,
                *init,
                *loop_var_index,
                body,
                names,
                states,
                i64_names,
            );
            let mut body_i64 = i64_names.clone();
            if i64_fold {
                body_i64.insert(loop_name.clone());
            }
            let outer_fold = fold_context();
            set_fold_context(true);
            let body_expr = value_expr(body, &body_names, states, &body_i64);
            set_fold_context(outer_fold);
            let body_code = render_expr(&body_expr?);
            let init_code = render_expr(&operand(program, *init));
            let start_code = render_expr(&operand(program, *start));
            let end_code = render_expr(&operand(program, *end));
            let update = match combine {
                FoldCombine::Add if i64_fold => {
                    "__acc = __acc.checked_add(__item).expect(\"i64 overflow\");"
                }
                FoldCombine::Mul if i64_fold => {
                    "__acc = __acc.checked_mul(__item).expect(\"i64 overflow\");"
                }
                FoldCombine::Add => "__acc += __item;",
                FoldCombine::Mul => "__acc *= __item;",
                FoldCombine::And => "__acc = __acc && __item;",
                FoldCombine::Or => "__acc = __acc || __item;",
            };
            Ok(Expr::Raw(format!(
                "{{ let mut __acc = {init_code}; for {loop_name} in (({start_code}) as i64)..(({end_code}) as i64) {{ let __item = {body_code}; {update} }} __acc }}"
            )))
        }
        EmirOp::OptionSome(payload) => {
            let tys = carrier_payload_types(program, names, states, i64_names)?;
            let idx = op_self_index(program, op).unwrap_or(u32::MAX);
            Ok(Expr::Raw(format!(
                "Option::<{}>::Some({})",
                tys.option(idx),
                render_expr(&operand(program, *payload))
            )))
        }
        EmirOp::OptionNone => {
            let tys = carrier_payload_types(program, names, states, i64_names)?;
            let idx = op_self_index(program, op).unwrap_or(u32::MAX);
            Ok(Expr::Raw(format!("Option::<{}>::None", tys.option(idx))))
        }
        EmirOp::OptionIsSome(carrier) => {
            expect_carrier(program, *carrier, false, op.name())?;
            Ok(Expr::Raw(format!(
                "{}.is_some()",
                render_expr(&operand(program, *carrier))
            )))
        }
        EmirOp::OptionUnwrapOr(carrier, default) => {
            expect_carrier(program, *carrier, false, op.name())?;
            Ok(Expr::Raw(format!(
                "{}.unwrap_or({})",
                render_expr(&operand(program, *carrier)),
                render_expr(&operand(program, *default))
            )))
        }
        EmirOp::ResultOk(payload) => {
            let tys = carrier_payload_types(program, names, states, i64_names)?;
            let idx = op_self_index(program, op).unwrap_or(u32::MAX);
            Ok(Expr::Raw(format!(
                "Result::<{}, {}>::Ok({})",
                tys.result_ok(idx),
                tys.result_err(idx),
                render_expr(&operand(program, *payload))
            )))
        }
        EmirOp::ResultErr(payload) => {
            let tys = carrier_payload_types(program, names, states, i64_names)?;
            let idx = op_self_index(program, op).unwrap_or(u32::MAX);
            Ok(Expr::Raw(format!(
                "Result::<{}, {}>::Err({})",
                tys.result_ok(idx),
                tys.result_err(idx),
                render_expr(&operand(program, *payload))
            )))
        }
        EmirOp::ResultIsOk(carrier) => {
            expect_carrier(program, *carrier, true, op.name())?;
            Ok(Expr::Raw(format!(
                "{}.is_ok()",
                render_expr(&operand(program, *carrier))
            )))
        }
        EmirOp::ResultUnwrapOr(carrier, default) => {
            expect_carrier(program, *carrier, true, op.name())?;
            Ok(Expr::Raw(format!(
                "{}.unwrap_or({})",
                render_expr(&operand(program, *carrier)),
                render_expr(&operand(program, *default))
            )))
        }
        EmirOp::ResultErrorOf(carrier) => {
            expect_carrier(program, *carrier, true, op.name())?;
            let tys = carrier_payload_types(program, names, states, i64_names)?;
            let err_ty = tys.result_err(carrier.0);
            Ok(Expr::Raw(format!(
                "match {} {{ Ok(_) => Option::<{err_ty}>::None, Err(__opt_err) => Option::<{err_ty}>::Some(__opt_err) }}",
                render_expr(&operand(program, *carrier))
            )))
        }
        _ => unreachable!("op_flow_exprs routed a non-universal control op"),
    }
}
