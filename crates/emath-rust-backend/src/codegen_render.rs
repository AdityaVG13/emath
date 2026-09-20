use std::collections::{BTreeMap, HashMap};

use crate::rust_ir::ast::{BinOp, Block, Expr, Stmt, Ty, UnOp, escape_ident};
use crate::rust_ir::render::render_expr;
use emath_exec_ir::optimize::{is_total, operand_registers};
use emath_exec_ir::{EmirOp, EmirProgram, EmirSliceAxis, EmirValue, FoldCombine};

use crate::BackendError;
use crate::codegen_helpers::comparison;
use crate::contains_call_self;

mod op_arith;
mod op_collections;
mod op_data;
mod op_flow;

use op_arith::op_arith_exprs;
use op_collections::op_collection_exprs;
use op_data::op_data_exprs;
use op_flow::{op_flow_exprs, authored_control_expr};

mod carrier;
mod flat;
mod kinds;
pub(crate) mod kernels;
mod record_layouts;
mod rtcalls;

use carrier::*;
pub(crate) use flat::*;
pub(crate) use kinds::*;
pub(crate) use kernels::element_tensor_expr;
pub(crate) use record_layouts::{record_layout, AuthoredRecordScope};
pub(crate) use rtcalls::*;

/// Wrap an operand expression into the value union by its kind. Total
/// over the joinable kinds (union, Int, Rat, Bool): the union-lane
/// kind rules only admit those beside a CodeValue operand, so the
/// `unreachable` arm is a compile-time invariant, not a runtime path.
pub(crate) fn to_code_value(expr: Expr, kind: &ValueKind) -> Expr {
    match kind {
        ValueKind::CodeValue => expr,
        ValueKind::I64 => Expr::Raw(format!(
            "emath_rt::code::CodeValue::Int({})",
            render_expr(&expr)
        )),
        ValueKind::Rational => Expr::Raw(format!(
            "emath_rt::code::CodeValue::Rat({})",
            render_expr(&expr)
        )),
        ValueKind::Bool => Expr::Raw(format!(
            "emath_rt::code::CodeValue::Bool({})",
            render_expr(&expr)
        )),
        other => unreachable!(
            "the union-lane kind rules admit only Int/Rat/Bool beside the union, found {other:?}"
        ),
    }
}

pub(crate) fn op_expr(
    op: &EmirOp,
    program: &EmirProgram,
    names: &[String],
    states: &[String],
    input_kinds: &InputKinds,
) -> Result<Expr, BackendError> {
    match op {
        EmirOp::CallFrame { body, inputs, state, declared } => {
            let kinds = value_kinds(program, names, states, input_kinds);
            literal_frame_expr(body, inputs, state, program, &kinds, declared)
        }
        EmirOp::CallSelf { inputs, .. } => {
            // The recursive target (`__self` at entries, `__frame_self`
            // in inlined frames) declares owned parameters for
            // non-copy carriers and `&dyn Fn` for closures. Arguments
            // follow: records and vectors re-materialize owned
            // (clone), closure carriers rest as `Rc<dyn Fn>` registers
            // so one borrow coerces to the `&dyn Fn` parameter.
            let kinds = value_kinds(program, names, states, input_kinds);
            let args = inputs
                .iter()
                .map(|value| {
                    let kind = kind_at(&kinds, *value);
                    if matches!(kind, ValueKind::Closure { .. }) {
                        // Closure carriers are shared `Rc` handles:
                        // clone into the `Rc<dyn Fn>` parameter.
                        format!("{}.clone()", render_expr(&operand(program, *value)))
                    } else if kind.is_copy() {
                        render_expr(&operand(program, *value))
                    } else {
                        render_expr(&owned_operand(program, *value, &kinds))
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            Ok(Expr::Raw(format!("__self({args})?")))
        }
        EmirOp::SameDenseShape(..) | EmirOp::DenseValues(_) | EmirOp::DenseRepack { .. } | EmirOp::ToF64(_)
        | EmirOp::DenseLayout(_) | EmirOp::VectorSlice { .. } | EmirOp::VectorConcat(_) | EmirOp::ListConcat(_) | EmirOp::F64SortTotal(_) => {
            let kinds = value_kinds(program, names, states, input_kinds);
            op_collection_exprs(op, program, &kinds)
        }
        EmirOp::Branch { .. } | EmirOp::Iterate { .. } | EmirOp::Collect { .. } | EmirOp::Refuse(_) | EmirOp::RefuseValue(_) => authored_control_expr(op, program, names, states, input_kinds),
        EmirOp::ListCreate(elements) => {
            let kinds = value_kinds(program, names, states, input_kinds);
            Ok(Expr::Macro { name: "vec".into(), args: elements.iter().map(|value| owned_operand(program, *value, &kinds)).collect() })
        }
        EmirOp::RecordField { record, field } => {
            let value = Expr::Field { receiver: Box::new(operand(program, *record)), field: escape_ident(field) };
            let kinds = value_kinds(program, names, states, input_kinds);
            if kind_of_op(op, &kinds, names, states, input_kinds).is_copy() { Ok(value) } else { Ok(Expr::Raw(format!("&{}", render_expr(&value)))) }
        }
        EmirOp::ToInt(value) => {
            let kinds = value_kinds(program, names, states, input_kinds);
            checked_integer_operand(program, *value, &kinds)
        }
        EmirOp::IntegerQuotient(left, right) => {
            let kinds = value_kinds(program, names, states, input_kinds);
            let left_k = kind_at(&kinds, *left);
            let right_k = kind_at(&kinds, *right);
            if left_k == ValueKind::I64 && right_k == ValueKind::I64 {
                return Ok(checked_integer_result(Expr::MethodCall {
                    receiver: Box::new(operand(program, *left)),
                    method: "checked_div_euclid".to_string(),
                    args: vec![operand(program, *right)],
                }));
            }
            if matches!(left_k, ValueKind::I64 | ValueKind::ExactInt)
                && matches!(right_k, ValueKind::I64 | ValueKind::ExactInt)
            {
                return Ok(map_runtime_result(format!(
                    "{}.quot(&{}).map_err(|err| err.to_string())",
                    render_expr(&exact_int_operand(program, *left, &kinds)),
                    render_expr(&exact_int_operand(program, *right, &kinds))
                )));
            }
            Err(BackendError::UnsupportedType(
                "quotient requires two Int operands".into(),
            ))
        }
        EmirOp::SameBits(left, right) => {
            let kinds = value_kinds(program, names, states, input_kinds);
            Ok(Expr::Raw(format!("({}).to_bits() == ({}).to_bits()", render_expr(&typed_operand(program, *left, ValueKind::F64, &kinds)), render_expr(&typed_operand(program, *right, ValueKind::F64, &kinds)))))
        }
        EmirOp::ConstF64(bits) => Ok(Expr::F64(*bits)),
        EmirOp::ConstI64(value) => Ok(Expr::Int(*value)),
        EmirOp::ConstExactInt(digits) => Ok(Expr::Raw(format!(
            "emath_rt::ExactInt::parse(\"{digits}\").expect(\"const-exact-int digits\")"
        ))),
        EmirOp::ExactIntCall { name, args } => {
            let kinds = value_kinds(program, names, states, input_kinds);
            Ok(exact_int_call_expr(name, args, program, &kinds)?)
        },
        EmirOp::ConstBigInt(digits) => Ok(Expr::Raw(format!(
            "emath_rt::UBig::parse_decimal(\"{digits}\").expect(\"const-bigint digits\")"
        ))),
        EmirOp::ConstText(value) => Ok(Expr::Str(value.clone())),
        EmirOp::ConstComplex(..)
        | EmirOp::ConstBool(..)
        | EmirOp::LoadInput(..)
        | EmirOp::LoadState(..)
        | EmirOp::FormatText { .. }
        | EmirOp::SeriesCreate { .. }
        | EmirOp::SeriesSample { .. }
        | EmirOp::SetCreate { .. }
        | EmirOp::SetContains { .. }
        | EmirOp::RecordCreate { .. } => op_data_exprs(op, program, names, states, input_kinds),
        EmirOp::F64Add(..)
        | EmirOp::F64Sub(..)
        | EmirOp::F64Mul(..)
        | EmirOp::F64Div(..)
        | EmirOp::F64Pow(..)
        | EmirOp::Neg(..)
        | EmirOp::UnaryBuiltin(..)
        | EmirOp::BinaryBuiltin(..)
        | EmirOp::Lt(..)
        | EmirOp::Le(..)
        | EmirOp::Gt(..)
        | EmirOp::Ge(..)
        | EmirOp::Eq(..)
        | EmirOp::Ne(..)
        | EmirOp::And(..)
        | EmirOp::Or(..)
        | EmirOp::Imply(..)
        | EmirOp::Iff(..)
        | EmirOp::Not(..)
        | EmirOp::IsFinite(..) => op_arith_exprs(
            op,
            program,
            &value_kinds(program, names, states, input_kinds),
        ),
        EmirOp::Select { .. }
        | EmirOp::VectorCreate(..)
        | EmirOp::VectorLength(..)
        | EmirOp::MatrixCreate { .. }
        | EmirOp::MatrixRows(_)
        | EmirOp::MatrixCols(_)
        | EmirOp::MatrixPack { .. }
        | EmirOp::TensorShape(_)
        | EmirOp::F64Exp2(_)
        | EmirOp::F64PowI(..)
        | EmirOp::TextTrim(_)
        | EmirOp::TextLength(_)
        | EmirOp::IndexText(_)
        | EmirOp::TextByte(..)
        | EmirOp::FormatScientific(..)
        | EmirOp::ParseF64(_)
        | EmirOp::TensorPack { .. }
        | EmirOp::DenseIndex { .. }
        | EmirOp::TensorCreate { .. }
        | EmirOp::VectorIndex { .. }
        | EmirOp::MatrixIndex { .. }
        | EmirOp::TensorIndex { .. }
        | EmirOp::TensorSlice { .. } => op_collection_exprs(
            op,
            program,
            &value_kinds(program, names, states, input_kinds),
        ),
        EmirOp::Fold { .. }
        | EmirOp::OptionSome(..)
        | EmirOp::OptionNone
        | EmirOp::OptionIsSome(..)
        | EmirOp::OptionUnwrapOr(..)
        | EmirOp::ResultOk(..)
        | EmirOp::ResultErr(..)
        | EmirOp::ResultIsOk(..)
        | EmirOp::ResultUnwrapOr(..)
        | EmirOp::ResultErrorOf(..) => op_flow_exprs(
            op,
            program,
            names,
            states,
            input_kinds,
            &value_kinds(program, names, states, input_kinds),
        ),
        EmirOp::ApplyCapability {
            capability,
            class,
            args,
        } => capability_artifact_expr(
            capability,
            *class,
            args,
            program,
            &value_kinds(program, names, states, input_kinds),
        ),
        EmirOp::ToF64Vector(sequence) => {
            let kinds = value_kinds(program, names, states, input_kinds);
            if kind_at(&kinds, *sequence) != ValueKind::Vector(Box::new(ValueKind::F64)) {
                return Err(BackendError::UnsupportedType("vector packing requires Float64 elements".into()));
            }
            Ok(owned_operand(program, *sequence, &kinds))
        }
        EmirOp::CallProgram { program: callable, inputs }
        | EmirOp::CallScalarProgram { program: callable, inputs }
        | EmirOp::CallRealProgram { program: callable, inputs }
        | EmirOp::TryCallRealProgram { program: callable, inputs } => {
            let kinds = value_kinds(program, names, states, input_kinds);
            if kind_at(&kinds, *callable) != ValueKind::Program
                || kind_at(&kinds, *inputs) != ValueKind::Vector(Box::new(ValueKind::F64))
            {
                return Err(BackendError::UnsupportedType("numeric call requires Program and Vector<Float64>".into()));
            }
            let call = format!("({})(&{})", render_expr(&operand(program, *callable)), render_expr(&operand(program, *inputs)));
            Ok(match op {
                EmirOp::CallProgram { .. } => map_runtime_result(format!("{call}.and_then(emath_rt::NumericProgramResult::into_vector)")),
                EmirOp::CallScalarProgram { .. } => map_runtime_result(format!("{call}.and_then(emath_rt::NumericProgramResult::into_scalar)")),
                EmirOp::CallRealProgram { .. } => map_runtime_result(format!("{call}.and_then(emath_rt::NumericProgramResult::into_real)")),
                _ => Expr::Raw(format!("{call}.and_then(emath_rt::NumericProgramResult::into_real)")),
            })
        }
        EmirOp::CallValue { program: callee_value, inputs } => {
            // Typed indirect call on a closure carrier. The callee's
            // declared signature decides arity and borrowing: copy
            // arguments render owned, reference carriers borrow, and
            // the closure's own Result propagates with `?`. A curried
            // callee consumes its arguments one stage at a time -
            // `(f)(a)?` yields the next callable (boxed), which the
            // following stage calls.
            let kinds = value_kinds(program, names, states, input_kinds);
            let ValueKind::Closure { .. } = kind_at(&kinds, *callee_value) else {
                return Err(BackendError::UnsupportedType(
                    "closure call requires a callee with a declared signature".into(),
                ));
            };
            let callee = render_expr(&operand(program, *callee_value));
            let mut call = format!("({callee})");
            let mut remaining: &[EmirValue] = inputs;
            let mut kind = kind_at(&kinds, *callee_value);
            loop {
                let ValueKind::Closure { params, result } = kind else {
                    return Err(BackendError::UnsupportedType(
                        "closure call requires a callee with a declared signature".into(),
                    ));
                };
                if remaining.len() < params.len() {
                    return Err(BackendError::UnsupportedType(
                        "closure call argument count does not match the declared signature".into(),
                    ));
                }
                let (args, rest) = remaining.split_at(params.len());
                let rendered = args
                    .iter()
                    .zip(params.iter())
                    .map(|(value, param)| {
                        let expression = render_expr(&operand(program, *value));
                        if param.is_copy() {
                            return expression;
                        }
                        // One reference layer exactly for data carriers:
                        // a LoadInput / LoadState register already
                        // rests as a borrowed binding, every other
                        // producer is owned (`&&T -> &T` never coerces
                        // at call arguments). Closure carriers are
                        // shared `Rc` handles and clone into the
                        // `Rc<dyn Fn>` parameter.
                        if matches!(param, ValueKind::Closure { .. }) {
                            return format!("{expression}.clone()");
                        }
                        let already_borrowed = program
                            .ops
                            .get(value.0 as usize)
                            .map(|(op, _)| {
                                matches!(op, EmirOp::LoadInput(_) | EmirOp::LoadState(_))
                            })
                            .unwrap_or(false);
                        if already_borrowed { expression } else { format!("&{expression}") }
                    })
                    .collect::<Vec<_>>();
                call = format!("({call}({})?)", rendered.join(", "));
                if rest.is_empty() {
                    break;
                }
                kind = *result;
                remaining = rest;
            }
            Ok(Expr::Raw(call))
        }
        EmirOp::ProgramLiteral { body, captures, vector_input, signature } => {
            if body.state_count != 0 {
                return Err(BackendError::UnsupportedType("program literal must be closed over state".into()));
            }
            let parameters = (0..body.input_count).map(|index| format!("__program_arg_{index}")).collect::<Vec<_>>();
            let explicit = usize::from(body.input_count).checked_sub(captures.len())
                .ok_or_else(|| BackendError::UnsupportedType("program captures exceed input count".into()))?;
            let kinds = value_kinds(program, names, states, input_kinds);
            if !signature.is_empty() {
                // Typed literal: the authored parameter domain fixes the
                // callable ABI. One explicit parameter, owned captures
                // moved into the closure, body ops propagate with `?`,
                // and the inferred result kind types the return.
                if *vector_input {
                    return Err(BackendError::UnsupportedType("vector program carrier is the numeric lane only".into()));
                }
                if explicit != 1 {
                    return Err(BackendError::UnsupportedType("typed program literal takes exactly one domain parameter".into()));
                }
                let param_kind = ValueKind::from_signature(signature);
                if matches!(param_kind, ValueKind::Other) {
                    return Err(BackendError::UnsupportedType(format!("program literal parameter domain {signature} has no native carrier")));
                }
                let param_ty = {
                    if matches!(param_kind, ValueKind::Closure { .. }) {
                        // A closure parameter's rust type already
                        // carries the `&dyn Fn` layer; borrowing again
                        // would double it.
                        crate::rust_ir::render::render_ty(&param_kind.rust_ty()?)
                    } else {
                        let ty = crate::rust_ir::render::render_ty(&param_kind.rust_ty()?);
                        if param_kind.is_copy() { ty } else { format!("&{ty}") }
                    }
                };
                let mut inputs = InputKinds::new();
                let mut capture_bindings = String::new();
                let mut bindings = String::new();
                inputs.insert(parameters[0].clone(), param_kind);
                for (index, name) in parameters.iter().enumerate().skip(1) {
                    let argument = captures[index - 1];
                    let kind = kind_at(&kinds, argument);
                    if let ValueKind::Closure { params, result } = &kind {
                        // A captured callable must be SHARED into the
                        // literal's `'static` carrier: borrowing the
                        // producing register would dangle once the
                        // carrier outlives it (E0515/E0597), and a
                        // `move` would break multi-use registers.
                        // Every closure carrier is a shared `Rc`
                        // handle, so the capture (and the per-call
                        // parameter binding) clone the handle.
                        capture_bindings.push_str(&format!(
                            "let __program_capture_{index}: std::rc::Rc<{}> = {}.clone(); ",
                            callable_ty(params, result)?,
                            render_expr(&operand(program, argument))
                        ));
                        bindings.push_str(&format!(
                            "let {name}: std::rc::Rc<{}> = __program_capture_{index}.clone(); ",
                            callable_ty(params, result)?
                        ));
                        inputs.insert(name.clone(), kind);
                        continue;
                    }
                    let value = render_expr(&owned_operand(program, argument, &kinds));
                    capture_bindings.push_str(&format!("let __program_capture_{index} = {value}; "));
                    let borrow = if kind.is_copy() { "" } else { "&" };
                    bindings.push_str(&format!("let {name} = {borrow}__program_capture_{index}; "));
                    inputs.insert(name.clone(), kind);
                }
                let _reference = ReferenceScope::enter();
                let body_code = render_expr(&value_expr(body, &parameters, &[], &inputs)?);
                let result_kind = program_kind(body, &parameters, &[], &inputs);
                let result_ty = match &result_kind {
                    ValueKind::Never => String::from("()"),
                    ValueKind::Other => {
                        let result_op = body
                            .ops
                            .get(body.result.0 as usize)
                            .map(|(op, _)| op.name().to_string())
                            .unwrap_or_else(|| "?".into());
                        return Err(BackendError::UnsupportedType(format!(
                            "program literal result has no native carrier (parameter domain {signature}, result op {result_op})"
                        )));
                    }
                    // A curried literal returns the next callable in
                    // SHARED `Rc<dyn Fn>` storage: the stage may be
                    // captured by further literals and called many
                    // times, and a borrowed closure cannot cross the
                    // `'static` carrier boundary.
                    ValueKind::Closure { params, result } => {
                        format!("std::rc::Rc<{}>", callable_ty(params, result)?)
                    }
                    kind => crate::rust_ir::render::render_ty(&kind.rust_ty()?),
                };
                // A curried literal returns the next callable, which
                // the inner literal already renders as shared
                // `Rc<dyn Fn>`; the literal value itself rests in an
                // `Rc` too, so every carrier position is callable
                // through any reference layer and capturable by
                // further literals without dangling borrows.
                let tail = if result_kind == ValueKind::Never {
                    body_code
                } else {
                    format!("Ok({body_code})")
                };
                Ok(Expr::Raw(format!(
                    "{{ {capture_bindings} std::rc::Rc::new(move |{}: {param_ty}| -> Result<{result_ty}, String> {{ {bindings} {tail} }}) }}",
                    parameters[0]
                )))
            } else {
            if *vector_input && explicit != 1 {
                return Err(BackendError::UnsupportedType("vector program requires exactly one explicit input".into()));
            }
            let mut inputs = InputKinds::new();
            let mut capture_bindings = String::new();
            let mut bindings = String::new();
            for (index, name) in parameters.iter().enumerate() {
                let kind = if index < explicit {
                    if *vector_input {
                        bindings.push_str(&format!("let {name} = __program_inputs.to_vec(); "));
                        ValueKind::Vector(Box::new(ValueKind::F64))
                    } else {
                        bindings.push_str(&format!("let {name} = __program_inputs[{index}]; "));
                        ValueKind::F64
                    }
                } else {
                    let argument = captures[index - explicit];
                    let kind = kind_at(&kinds, argument);
                    let value = render_expr(&owned_operand(program, argument, &kinds));
                    capture_bindings.push_str(&format!("let __program_capture_{index} = {value}; "));
                    let borrow = if kind.is_copy() { "" } else { "&" };
                    bindings.push_str(&format!("let {name} = {borrow}__program_capture_{index}; "));
                    kind
                };
                inputs.insert(name.clone(), kind);
            }
            let _reference = ReferenceScope::enter();
            let body_code = render_expr(&value_expr(body, &parameters, &[], &inputs)?);
            let result = match program_kind(body, &parameters, &[], &inputs) {
                ValueKind::F64 => format!("emath_rt::NumericProgramResult::Scalar({body_code})"),
                ValueKind::I64 => format!("emath_rt::NumericProgramResult::Integer({body_code})"),
                ValueKind::Never => body_code,
                ValueKind::Vector(element) if *element == ValueKind::F64 => format!("emath_rt::NumericProgramResult::Vector({body_code})"),
                _ => return Err(BackendError::UnsupportedType("numeric program must return Float64, Int, or Vector<Float64>".into())),
            };
            let arity_guard = if *vector_input { String::new() } else { format!("if __program_inputs.len() != {explicit} {{ return Err(String::from(\"call-program: argument count mismatch\")); }} ") };
            Ok(Expr::Raw(format!("{{ {capture_bindings} let __program: std::sync::Arc<dyn Fn(&[f64]) -> Result<emath_rt::NumericProgramResult, String>> = std::sync::Arc::new(move |__program_inputs: &[f64]| {{ {arity_guard} {bindings} Ok({result}) }}); __program }}")))
            }
        }
        EmirOp::CodeLiteral { body, param, free, carrier } => {
            if param.is_none() {
                // The compiled EXPRESSION template (bead
                // emath-expression-quotes-324y0): the quoted
                // expression lowers once over the value union - every
                // scalar op of the body renders as a dynamic rt kernel
                // call implementing the VM's exact carrier rules, no
                // tree carried and no interpreter run. All free names
                // are runtime inputs (the hygiene law keeps them
                // open); the factory computes the union-valued
                // expression directly - no parameter, no inner
                // closure. The body-kind check is the named gate:
                // anything the union lane cannot compute (structured
                // values, floats, non-scalar ops, a closed body with
                // no free name to carry the union) refuses here.
                let mut inputs = InputKinds::new();
                for name in free {
                    inputs.insert(name.clone(), ValueKind::CodeValue);
                }
                if program_kind(body, free, &[], &inputs) != ValueKind::CodeValue {
                    return Err(BackendError::UnsupportedType(
                        "quote expression template body must compute the scalar union (Int/Rat/Bool arithmetic, comparisons, and boolean combinators over its free names)".into(),
                    ));
                }
                let _reference = ReferenceScope::enter();
                let body_code = render_expr(&value_expr(body, free, &[], &inputs)?);
                let bindings = free
                    .iter()
                    .enumerate()
                    .map(|(index, name)| format!("let {name} = __code_free[{index}]; "))
                    .collect::<String>();
                let free_list = free
                    .iter()
                    .map(|name| format!("String::from({name:?})"))
                    .collect::<Vec<_>>()
                    .join(", ");
                return Ok(Expr::Raw(format!(
                    "{{ emath_rt::code::open_expr(vec![{free_list}], std::rc::Rc::new(move |__code_free: &[emath_rt::code::CodeValue]| -> Result<emath_rt::code::CodeValue, String> {{ {bindings} Ok({body_code}) }})) }}"
                )));
            }
            // The compiled FUNCTION template: the nested program lowers once
            // into a two-stage factory - the outer closure consumes
            // the open constants' values (in `free` order, as a
            // slice), the inner closure is the specialized unary
            // program. The nested inputs are [param, free...] -
            // positional like every EmirProgram - so the authored
            // names bind the closure parameters in that order. The
            // declared domain is the carrier: it governs the
            // parameter and the constants together, so the factory
            // instantiates `emath_rt::code::Code<V>` monomorphically
            // and a body computing any other carrier refuses typed.
            // No tree, no interpreter: candidates execute as this
            // emitted closure.
            let param = param.clone().unwrap_or_default();
            let carrier_kind = ValueKind::from_signature(carrier);
            if matches!(carrier_kind, ValueKind::Other) {
                return Err(BackendError::UnsupportedType(format!(
                    "quote template carrier `{carrier}` has no native scalar instantiation"
                )));
            }
            let mut parameters = vec![param.clone()];
            parameters.extend(free.iter().cloned());
            let mut inputs = InputKinds::new();
            for name in &parameters {
                inputs.insert(name.clone(), carrier_kind.clone());
            }
            if program_kind(body, &parameters, &[], &inputs) != carrier_kind {
                return Err(BackendError::UnsupportedType(format!(
                    "quote template body must compute its declared `{carrier}` carrier"
                )));
            }
            let _reference = ReferenceScope::enter();
            let body_code = render_expr(&value_expr(body, &parameters, &[], &inputs)?);
            // The nested body's result is a plain value; the closure
            // returns Result (a Never body - an authored refusal -
            // already renders as the error tail, mirroring the typed
            // literal's law).
            let tail = if program_kind(body, &parameters, &[], &inputs) == ValueKind::Never {
                body_code
            } else {
                format!("Ok({body_code})")
            };
            let carrier_ty =
                crate::rust_ir::render::render_ty(&carrier_kind.rust_ty()?);
            let unary_ty = format!(
                "std::rc::Rc<dyn Fn({carrier_ty}) -> Result<{carrier_ty}, String>>"
            );
            let closure_params = parameters
                .iter()
                .map(|name| format!("{name}: {carrier_ty}"))
                .collect::<Vec<_>>()
                .join(", ");
            let bindings = free
                .iter()
                .enumerate()
                .map(|(index, name)| format!("let {name} = __code_free[{index}]; "))
                .collect::<String>();
            // The inner closure owns the bound free values (Copy
            // tuples) and forwards them with the parameter.
            let mut forwarded = vec![param.clone()];
            forwarded.extend(free.iter().cloned());
            let forwarded = forwarded.join(", ");
            let free_list = free
                .iter()
                .map(|name| format!("String::from({name:?})"))
                .collect::<Vec<_>>()
                .join(", ");
            Ok(Expr::Raw(format!(
                "{{ let __code_body = move |{closure_params}| -> Result<{carrier_ty}, String> {{ {tail} }}; \
                 emath_rt::code::open::<{carrier_ty}>(vec![{free_list}], std::rc::Rc::new(move |__code_free: &[{carrier_ty}]| -> Result<{unary_ty}, String> {{ \
                 {bindings} let __code_unary: {unary_ty} = std::rc::Rc::new(move |{param}: {carrier_ty}| __code_body({forwarded})); \
                 Ok(__code_unary) }})) }}"
            )))
        }
        EmirOp::CodeSubstitute { code, reference, value } => {
            // Partial application of one open name. The reference
            // resolved statically at lowering. A FUNCTION template
            // demands the declared carrier (the carrier check is
            // named at emission; the generic instantiation would not
            // compile a mismatch anyway). An EXPRESSION template
            // accepts any scalar - the substitute-time carrier fact -
            // converted into the union.
            let kinds = value_kinds(program, names, states, input_kinds);
            match kind_at(&kinds, *code) {
                ValueKind::Code(carrier) => {
                    let value_kind = kind_at(&kinds, *value);
                    if value_kind != *carrier {
                        let name = |kind: &ValueKind| match kind {
                            ValueKind::Rational => "Rat".to_string(),
                            ValueKind::I64 => "Int".to_string(),
                            ValueKind::Bool => "Bool".to_string(),
                            other => format!("{other:?}"),
                        };
                        return Err(BackendError::UnsupportedType(format!(
                            "code-substitute value must be {}-carried to match the template's carrier {}",
                            name(&value_kind),
                            name(carrier.as_ref())
                        )));
                    }
                    Ok(Expr::Raw(format!(
                        "emath_rt::code::substitute(&{}, {:?}, {})",
                        render_expr(&operand(program, *code)),
                        reference,
                        render_expr(&operand(program, *value))
                    )))
                }
                ValueKind::ExprCode => {
                    let value_kind = kind_at(&kinds, *value);
                    if !union_promotable(&value_kind) || value_kind == ValueKind::CodeValue {
                        let name = |kind: &ValueKind| match kind {
                            ValueKind::Rational => "Rat".to_string(),
                            ValueKind::I64 => "Int".to_string(),
                            ValueKind::Bool => "Bool".to_string(),
                            other => format!("{other:?}"),
                        };
                        return Err(BackendError::UnsupportedType(format!(
                            "code-substitute into an expression template requires a scalar value, found {}",
                            name(&value_kind)
                        )));
                    }
                    let converted = to_code_value(operand(program, *value), &value_kind);
                    Ok(Expr::Raw(format!(
                        "emath_rt::code::substitute_expr(&{}, {:?}, {})",
                        render_expr(&operand(program, *code)),
                        reference,
                        render_expr(&converted)
                    )))
                }
                _ => Err(BackendError::UnsupportedType(
                    "code-substitute requires a Code value".into(),
                )),
            }
        }
        EmirOp::CodeEvaluate { code } => {
            // The guarded executor: open code refuses `unbound_code`
            // at runtime, naming the remaining names. Closed code
            // yields the specialized closure for a function template
            // (kind Closure) or the computed union scalar for an
            // expression template (kind CodeValue, projected at the
            // typed boundary).
            let kinds = value_kinds(program, names, states, input_kinds);
            match kind_at(&kinds, *code) {
                ValueKind::Code(_) => Ok(Expr::Raw(format!(
                    "emath_rt::code::evaluate(&{})?",
                    render_expr(&operand(program, *code))
                ))),
                ValueKind::ExprCode => Ok(Expr::Raw(format!(
                    "emath_rt::code::evaluate_expr(&{})?",
                    render_expr(&operand(program, *code))
                ))),
                _ => Err(BackendError::UnsupportedType(
                    "code-evaluate requires a Code value".into(),
                )),
            }
        }
        EmirOp::VectorMap { .. }
        | EmirOp::VectorMapScalar { .. }
        | EmirOp::VectorReduce { .. }
        | EmirOp::VectorAllFinite(_) => {
            Err(BackendError::MissingArtifactContract(op.name().to_string()))
        }
    }
}

/// Resolve the universal capability seam from artifact-contract data only.
/// Capability names remain opaque and are never used for backend dispatch.
fn capability_artifact_expr(
    capability: &str,
    class: emath_exec_ir::CellClass,
    args: &[EmirValue],
    program: &EmirProgram,
    kinds: &[ValueKind],
) -> Result<Expr, BackendError> {
    match class {
        emath_exec_ir::CellClass::Provider => Err(BackendError::UnsupportedBinding {
            capability: capability.to_string(),
            binding: "provider",
        }),
        emath_exec_ir::CellClass::Intrinsic => Err(BackendError::UnsupportedBinding {
            capability: capability.to_string(),
            binding: "native",
        }),
        emath_exec_ir::CellClass::Pure => {
            if emath_exec_ir::native_kernel::native_kernel(capability).is_none() {
                if let Some(cell) =
                    emath_exec_ir::native_kernel::installed_reference_cell(capability)
                {
                    if !cell.admits_arity(args.len())
                        || !cell.guards.is_empty()
                        || cell.result_guard.is_some()
                    {
                        return Err(BackendError::MissingArtifactContract(
                            capability.to_string(),
                        ));
                    }
                    let names = (0..cell.params.len())
                        .map(|index| format!("__reference_arg_{index}"))
                        .collect::<Vec<_>>();
                    let mut inputs = names.iter().zip(args)
                        .map(|(name, argument)| (name.clone(), kind_at(kinds, *argument)))
                        .collect();
                    let _reference = ReferenceScope::enter();
                    let mut source = String::from("{ ");
                    for (name, argument) in names.iter().zip(args) {
                        let kind = kind_at(kinds, *argument);
                        let value = render_expr(&operand(program, *argument));
                        if kind.is_copy() {
                            source.push_str(&format!("let {name} = {value}; "));
                        } else {
                            let ty = crate::rust_ir::render::render_ty(&kind.borrowed_rust_ty()?);
                            source.push_str(&format!("let {name}: &{ty} = &{value}; "));
                        }
                    }
                    let first_default = cell.params.len() - cell.defaults.len();
                    for index in args.len()..names.len() {
                        let default = &cell.defaults[index - first_default];
                        let code = render_expr(&value_expr(default, &names[..index], &[], &inputs)?);
                        let kind = program_kind(default, &names[..index], &[], &inputs);
                        source.push_str(&format!("let {} = {code}; ", names[index]));
                        inputs.insert(names[index].clone(), kind);
                    }
                    let body = render_expr(&value_expr(&cell.program, &names, &[], &inputs)?);
                    source.push_str(&body);
                    source.push_str(" }");
                    drop(_reference);
                    if rate_context() || fold_context() {
                        return Ok(map_runtime_result(format!("(|| -> Result<_, String> {{ Ok({source}) }})()")));
                    }
                    return Ok(Expr::Raw(source));
                }
            }
            let binding = emath_exec_ir::native_kernel::verified_kernel_binding(capability)
                .map_err(|error| match error {
                    emath_exec_ir::native_kernel::KernelBindingError::MissingBinding(_) => {
                        BackendError::MissingArtifactBinding(capability.to_string())
                    }
                    _ => BackendError::StaleArtifactBinding(capability.to_string()),
                })?;
            if let Some(artifact) = emath_exec_ir::native_kernel::checked::verified(capability) {
                let inputs = binding
                    .signature
                    .strip_prefix('(')
                    .and_then(|s| s.split_once(")->"))
                    .map(|(s, _)| s)
                    .unwrap_or("");
                let types: Vec<_> = inputs.split(',').collect();
                let arguments = args
                    .iter()
                    .enumerate()
                    .map(|(index, arg)| {
                        let value = render_expr(&operand(program, *arg));
                        if artifact.borrowed & (1 << index) != 0 {
                            format!("&{value}")
                        } else if types.get(index) == Some(&"Float64") {
                            format!("({value}) as f64")
                        } else {
                            value
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                return Ok(map_runtime_result(format!(
                    "emath_rt::{}({arguments})", artifact.rust_function
                )));
            }
            let artifact = KERNEL_ARTIFACTS
                .iter()
                .find(|artifact| {
                    artifact.kernel_id == binding.kernel_id
                        && artifact.signature == binding.signature
                        && artifact.semantic_hash == binding.semantic_hash
                })
                .ok_or_else(|| BackendError::StaleArtifactBinding(capability.to_string()))?;
            artifact
                .render(args, program, kinds)
                .ok_or_else(|| BackendError::UnsupportedType(format!(
                    "kernel {} refuses argument carriers {:?}",
                    artifact.kernel_id,
                    args.iter().map(|arg| kind_at(kinds, *arg)).collect::<Vec<_>>()
                )))
        }
        _ => Err(BackendError::MissingArtifactContract(
            capability.to_string(),
        )),
    }
}

struct KernelArtifact {
    kernel_id: &'static str,
    signature: &'static str,
    semantic_hash: &'static str,
    kind: KernelArtifactKind,
}

enum KernelArtifactKind {
    DensePointIndex,
    CheckedAdd,
    ForwardDifference,
    EinsumContract,
    RsEncode,
    UnitInterval,
}

impl KernelArtifactKind {
    /// Whether the artifact's rendered form can produce a runtime fault
    /// (`Result`-typed render).
    fn faults(&self) -> bool {
        true
    }
}

/// Whether a capability's codegen artifact can produce a runtime fault,
/// for `program_may_fault` (the goal return's `Result` decision).
pub(super) fn artifact_may_fault(capability: &str) -> bool {
    let Ok(binding) =
        emath_exec_ir::native_kernel::verified_kernel_binding(capability)
    else {
        return false;
    };
    KERNEL_ARTIFACTS.iter().any(|artifact| {
        artifact.kernel_id == binding.kernel_id
            && artifact.signature == binding.signature
            && artifact.semantic_hash == binding.semantic_hash
            && artifact.kind.faults()
    })
}

impl KernelArtifact {
    fn render(&self, args: &[EmirValue], program: &EmirProgram, kinds: &[ValueKind]) -> Option<Expr> {
        match self.kind {
            KernelArtifactKind::DensePointIndex => op_collections::dense_point_index_expr(args, program, kinds),
            KernelArtifactKind::CheckedAdd => {
                let [left, right] = args else {
                    return None;
                };
                let left = render_expr(&operand_ref(program, *left));
                let right = render_expr(&operand_ref(program, *right));
                // Same typed fault as the interp handler (`checked_add` in
                // exec-ir native_kernel.rs): generated code matches the VM.
                Some(Expr::Raw(format!(
                    "({left}).checked_add({right}).ok_or_else(|| String::from(\"E-ARITH-OVERFLOW: checked integer addition overflowed\"))?"
                )))
            }
            KernelArtifactKind::ForwardDifference => {
                kernels::forward_difference_expr(args, program, kinds)
            }
            KernelArtifactKind::EinsumContract => {
                kernels::einsum_contract_expr(args, program, kinds)
            }
            KernelArtifactKind::RsEncode => kernels::rs_encode_expr(args, program, kinds),
            KernelArtifactKind::UnitInterval => {
                let [seed, draws, tail @ ..] = args else {
                    return None;
                };
                let stream = match tail {
                    [] => "\"\"".to_string(),
                    [stream] => format!("&{}", render_expr(&operand(program, *stream))),
                    _ => return None,
                };
                Some(map_runtime_result(format!(
                    "emath_rt::unit_interval_stream({}, {}, {stream})",
                    render_expr(&operand(program, *seed)),
                    render_expr(&operand(program, *draws)),
                )))
            }
        }
    }
}

const KERNEL_ARTIFACTS: &[KernelArtifact] = &[
    KernelArtifact {
        kernel_id: "checked-add",
        signature: "(Int,Int)->Int",
        semantic_hash: "sha256:79bdccd71ba3fc3c6419e05dae510273992460767e6fee5927baee1318a8e801",
        kind: KernelArtifactKind::CheckedAdd,
    },
    KernelArtifact {
        kernel_id: "checked-dense-index",
        signature: "(Dense<Float64>,Sequence)->Float64",
        semantic_hash: "sha256:c278f35784be8439c9141efaa734b85ce1cb9df8b479e8bc1b8dbe27561a0c8a",
        kind: KernelArtifactKind::DensePointIndex,
    },
    KernelArtifact {
        kernel_id: "counter-stream-unit-interval",
        signature: "(Float64,Float64,Text?)->Vector<Float64>",
        semantic_hash: "sha256:f054beaa11ce1ee6f33f2577538b9bfb979b93368f376c9cc2347fdc5699a134",
        kind: KernelArtifactKind::UnitInterval,
    },
    KernelArtifact {
        kernel_id: "program-forward-difference",
        signature: "(Program,Vector<Float64>,I64)->Float64",
        semantic_hash: "sha256:2d66f6359265754f8f0db969186c15af3ad9e7c616ba0ea57e7f5480c3c1c118",
        kind: KernelArtifactKind::ForwardDifference,
    },
    KernelArtifact {
        kernel_id: "einsum-contract",
        signature: "(Text,Sequence)->Tensor",
        semantic_hash: "sha256:b9586ff08f5ace59f2b7794c1ad138220b395c2da50b681f983939c2c24f14f8",
        kind: KernelArtifactKind::EinsumContract,
    },
    KernelArtifact {
        kernel_id: "modular-evaluation-sequence",
        signature: "(Vector<ExactInt>,Nat,PositiveExactInt)->Vector<ExactInt>",
        semantic_hash: "sha256:7dea40cb0d61f2dab74ee6d9d881c6c70bb052717bfca40a9b2ebd2acff8cac8",
        kind: KernelArtifactKind::RsEncode,
    },
];

fn literal_frame_expr(
    body: &EmirProgram,
    inputs: &[EmirValue],
    state: &[EmirValue],
    outer: &EmirProgram,
    kinds: &[ValueKind],
    declared: &[String],
) -> Result<Expr, BackendError> {
    if inputs.len() != usize::from(body.input_count) || state.len() != usize::from(body.state_count) {
        return Err(BackendError::UnsupportedType("frame argument count mismatch".into()));
    }
    let names = (0..inputs.len()).map(|index| format!("__frame_input_{index}")).collect::<Vec<_>>();
    let states = (0..state.len()).map(|index| format!("__frame_state_{index}")).collect::<Vec<_>>();
    let mut frame_kinds = InputKinds::new();
    let mut code = String::from("{ ");
    for (index, (name, value)) in names.iter().zip(inputs).enumerate() {
        let kind = frame_input_kind(kind_at(kinds, *value), declared.get(index));
        // A closure carrier binds as a clone of the shared `Rc<dyn
        // Fn>` handle: every closure carrier (register or scope
        // binding) is an `Rc`, so crossings clone and calls
        // auto-deref.
        if let ValueKind::Closure { params, result } = &kind {
            let source = render_expr(&operand(outer, *value));
            code.push_str(&format!(
                "let {name}: std::rc::Rc<{}> = {source}.clone(); ",
                callable_ty(params, result)?
            ));
            frame_kinds.insert(name.clone(), kind);
            continue;
        }
        let borrow = if kind.is_copy() { "" } else { "&" };
        code.push_str(&format!("let {name} = {borrow}{}; ", render_expr(&operand(outer, *value))));
        frame_kinds.insert(name.clone(), kind);
    }
    for (name, value) in states.iter().zip(state) {
        let kind = kind_at(kinds, *value);
        let borrow = if kind.is_copy() { "" } else { "&" };
        code.push_str(&format!("let {name} = {borrow}{}; ", render_expr(&operand(outer, *value))));
        frame_kinds.insert(name.clone(), kind);
    }
    let _reference = ReferenceScope::enter();
    let _state = LocalStateScope::enter();
    if contains_call_self(body) {
        // A sibling body that recurses on itself cannot inline: its
        // `__self` would bind the entry's wrapper with the wrong
        // signature. Render the frame as a local recursive fn over
        // the frame inputs instead - the same checked body, one
        // callable per frame. Frame lets above stay as the call's
        // argument bindings; the fn's parameters shadow them inside.
        let mut params = Vec::new();
        let mut call_args = Vec::new();
        for name in names.iter().chain(states.iter()) {
            let kind = frame_kinds.get(name).cloned().unwrap_or(ValueKind::Other);
            let ty = crate::rust_ir::render::render_ty(&kind.rust_ty()?);
            if matches!(kind, ValueKind::Closure { .. }) {
                // A closure parameter carries the shared `Rc<dyn Fn>`
                // handle; the initial call clones the frame binding
                // into it.
                params.push(format!("{name}: {ty}"));
                call_args.push(format!("{name}.clone()"));
            } else if kind.is_copy() {
                params.push(format!("{name}: {ty}"));
                call_args.push(name.clone());
            } else {
                // The recursive frame fn owns its non-copy carriers.
                // A frame binding is always a reference (one or two
                // layers: the operand may itself be a borrowed
                // register), so `&*` normalizes to exactly one layer
                // and the clone re-materializes the owned value for
                // the initial call. `CallSelf` re-materializes on
                // every recursive step the same way.
                params.push(format!("{name}: {ty}"));
                call_args.push(format!(
                    "<{ty} as Clone>::clone(&*{name})"
                ));
            }
        }
        let result_kind = program_kind(body, &names, &states, &frame_kinds);
        let result_ty = match result_kind.rust_ty() {
            Ok(ty) => crate::rust_ir::render::render_ty(&ty),
            Err(_) => "impl core::fmt::Debug".to_string(),
        };
        let mut body_code = render_expr(&value_expr(body, &names, &states, &frame_kinds)?)
            .replace("__self(", "__frame_self(");
        // A frame tail that resolves to a borrowed register (a
        // LoadInput binding) must return the owned carrier the
        // recursive fn's signature promises; `owned_value` handles
        // both owned and borrowed register shapes.
        if !result_kind.is_copy() && !matches!(result_kind, ValueKind::Closure { .. }) {
            body_code = render_expr(&owned_value(Expr::Raw(body_code), &result_kind));
        }
        code.push_str(&format!(
            "fn __frame_self({}) -> Result<{result_ty}, String> {{ Ok({body_code}) }} __frame_self({})? ",
            params.join(", "),
            call_args.join(", ")
        ));
        code.push_str(" }");
        return Ok(Expr::Raw(code));
    }
    code.push_str(&render_expr(&value_expr(body, &names, &states, &frame_kinds)?));
    code.push_str(" }");
    Ok(Expr::Raw(code))
}
