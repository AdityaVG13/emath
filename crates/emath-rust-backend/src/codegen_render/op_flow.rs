//! Universal fold and native Rust carrier lowering.

use super::*;

pub(super) fn op_flow_exprs(
    op: &EmirOp,
    program: &EmirProgram,
    names: &[String],
    states: &[String],
    input_kinds: &InputKinds,
    kinds: &[ValueKind],
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
                input_kinds,
            );
            let mut body_kinds = input_kinds.clone();
            if i64_fold {
                body_kinds.insert(loop_name.clone(), ValueKind::I64);
            }
            let outer_fold = fold_context();
            set_fold_context(true);
            let body_expr = value_expr(body, &body_names, states, &body_kinds);
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
            let tys = carrier_payload_types(program, names, states, input_kinds)?;
            let idx = op_self_index(program, op).unwrap_or(u32::MAX);
            Ok(Expr::Raw(format!(
                "Option::<{}>::Some({})",
                tys.option(idx),
                render_expr(&operand(program, *payload))
            )))
        }
        EmirOp::OptionNone => {
            let tys = carrier_payload_types(program, names, states, input_kinds)?;
            let idx = op_self_index(program, op).unwrap_or(u32::MAX);
            Ok(Expr::Raw(format!("Option::<{}>::None", tys.option(idx))))
        }
        EmirOp::OptionIsSome(carrier) => {
            expect_carrier(program, *carrier, false, op.name(), &kinds)?;
            Ok(Expr::Raw(format!(
                "{}.is_some()",
                render_expr(&operand(program, *carrier))
            )))
        }
        EmirOp::OptionUnwrapOr(carrier, default) => {
            expect_carrier(program, *carrier, false, op.name(), &kinds)?;
            Ok(Expr::Raw(format!(
                "{}.unwrap_or({})",
                render_expr(&operand(program, *carrier)),
                render_expr(&operand(program, *default))
            )))
        }
        EmirOp::ResultOk(payload) => {
            let tys = carrier_payload_types(program, names, states, input_kinds)?;
            let idx = op_self_index(program, op).unwrap_or(u32::MAX);
            Ok(Expr::Raw(format!(
                "Result::<{}, {}>::Ok({})",
                tys.result_ok(idx),
                tys.result_err(idx),
                render_expr(&operand(program, *payload))
            )))
        }
        EmirOp::ResultErr(payload) => {
            let tys = carrier_payload_types(program, names, states, input_kinds)?;
            let idx = op_self_index(program, op).unwrap_or(u32::MAX);
            Ok(Expr::Raw(format!(
                "Result::<{}, {}>::Err({})",
                tys.result_ok(idx),
                tys.result_err(idx),
                render_expr(&operand(program, *payload))
            )))
        }
        EmirOp::ResultIsOk(carrier) => {
            expect_carrier(program, *carrier, true, op.name(), &kinds)?;
            Ok(Expr::Raw(format!(
                "{}.is_ok()",
                render_expr(&operand(program, *carrier))
            )))
        }
        EmirOp::ResultUnwrapOr(carrier, default) => {
            expect_carrier(program, *carrier, true, op.name(), &kinds)?;
            Ok(Expr::Raw(format!(
                "{}.as_ref().map(|__value| (*__value).clone()).unwrap_or({})",
                render_expr(&operand(program, *carrier)),
                render_expr(&operand(program, *default))
            )))
        }
        EmirOp::ResultErrorOf(carrier) => {
            expect_carrier(program, *carrier, true, op.name(), &kinds)?;
            let tys = carrier_payload_types(program, names, states, input_kinds)?;
            let err_ty = tys.result_err(carrier.0);
            Ok(Expr::Raw(format!(
                "match {} {{ Ok(_) => Option::<{err_ty}>::None, Err(__opt_err) => Option::<{err_ty}>::Some(__opt_err) }}",
                render_expr(&operand(program, *carrier))
            )))
        }
        _ => unreachable!("op_flow_exprs routed a non-universal control op"),
    }
}


thread_local! { static CONTROL_DEPTH: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }
struct ControlScope(usize);
impl Drop for ControlScope {
    fn drop(&mut self) { CONTROL_DEPTH.with(|depth| depth.set(self.0)); }
}

/// Lazy source control retains error returns in the enclosing generated function.
pub(super) fn authored_control_expr(
    op: &EmirOp,
    program: &EmirProgram,
    names: &[String],
    states: &[String],
    inputs: &InputKinds,
) -> Result<Expr, BackendError> {
    let scope = ControlScope(CONTROL_DEPTH.with(|depth| { let previous = depth.get(); depth.set(previous + 1); previous }));
    let prefix = format!("__control{}_", scope.0);
    let kinds = value_kinds(program, names, states, inputs);
    let nested = |body: &EmirProgram, argument_kinds: Vec<ValueKind>| -> Result<String, BackendError> {
        let names = (0..argument_kinds.len()).map(|index| format!("{prefix}arg_{index}")).collect::<Vec<_>>();
        let inputs = names.iter().cloned().zip(argument_kinds).collect();
        Ok(render_expr(&value_expr(body, &names, &[], &inputs)?).replace("__e", &format!("{prefix}e")))
    };
    let captures = |args: &[EmirValue], start: usize| -> Result<String, BackendError> {
        let mut source = String::new();
        for (index, value) in args.iter().enumerate() {
            let name = format!("{prefix}arg_{}", start + index);
            let expression = render_expr(&operand(program, *value));
            let kind = kind_at(&kinds, *value);
            if kind.is_copy() { source.push_str(&format!("let {name} = {expression};")); }
            else if let ValueKind::Closure { params, result } = &kind {
                // A closure capture clones the shared `Rc<dyn Fn>`
                // handle; every closure carrier (register or scope
                // binding) is an `Rc`.
                source.push_str(&format!(
                    "let {name}: std::rc::Rc<{}> = {expression}.clone();",
                    callable_ty(params, result)?
                ));
            }
            else { source.push_str(&format!("let {name}: &{} = &{expression};", crate::rust_ir::render::render_ty(&kind.borrowed_rust_ty()?))); }
        }
        Ok(source)
    };
    Ok(Expr::Raw(match op {
        EmirOp::Refuse(detail) => format!("return Err({}.to_string())", render_expr(&Expr::Str(detail.clone()))),
        EmirOp::RefuseValue(value) => {
            if kind_at(&kinds, *value) != ValueKind::Text { return Err(BackendError::UnsupportedType("refusal detail must be Text".into())); }
            format!("return Err({}.to_string())", render_expr(&operand(program, *value)))
        }
        EmirOp::Branch { condition, args, then_body, else_body } => {
            let argument_kinds = args.iter().map(|value| kind_at(&kinds, *value)).collect::<Vec<_>>();
            let then_body = nested(then_body, argument_kinds.clone())?;
            let else_body = nested(else_body, argument_kinds)?;
            format!("{{ {} if {} {{ {then_body} }} else {{ {else_body} }} }}", captures(args, 0)?, render_expr(&operand(program, *condition)))
        }
        EmirOp::Collect { count, args, body } => {
            let argument_kinds = std::iter::once(ValueKind::I64).chain(args.iter().map(|value| kind_at(&kinds, *value))).collect();
            let body = nested(body, argument_kinds)?;
            format!("{{ let __collect_count = {}; let __collect_size = usize::try_from(__collect_count).map_err(|_| String::from(\"collection count must be nonnegative and fit usize\"))?; let mut __collect_values = Vec::new(); __collect_values.try_reserve_exact(__collect_size).map_err(|_| String::from(\"collection allocation exceeds available capacity\"))?; {} for {prefix}arg_0 in 0..__collect_count {{ __collect_values.push({body}); }} __collect_values }}", render_expr(&checked_integer_operand(program, *count, &kinds)?), captures(args, 1)?)
        }
        EmirOp::Iterate { count, init, args, stop, body } => {
            let state_kind = kind_at(&kinds, *init);
            let mut argument_kinds = vec![ValueKind::I64, state_kind.clone()];
            argument_kinds.extend(args.iter().map(|value| kind_at(&kinds, *value)));
            let stop = stop.as_ref().map(|stop| nested(stop, argument_kinds.clone())).transpose()?
                .map(|stop| format!("if {stop} {{ break; }}")).unwrap_or_default();
            let body = nested(body, argument_kinds)?;
            let borrow = if state_kind.is_copy() { "" } else { "&" };
            format!("{{ let __control_count = {}; if __control_count < 0 {{ return Err(String::from(\"iteration count must be nonnegative\")); }} let mut __control_state = {}; for __control_index in 0..__control_count {{ let {prefix}arg_0 = __control_index; let {prefix}arg_1 = {borrow}__control_state; {} {stop} __control_state = {body}; }} __control_state }}", render_expr(&checked_integer_operand(program, *count, &kinds)?), render_expr(&owned_operand(program, *init, &kinds)), captures(args, 2)?)
        }
        _ => unreachable!("authored control route"),
    }))
}
