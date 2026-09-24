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
use op_flow::{authored_control_expr, op_flow_exprs};

mod carrier;
mod flat;
pub(crate) mod kernels;
mod kinds;
mod record_layouts;
mod rtcalls;

use carrier::{carrier_payload_types, expect_carrier, op_self_index};
pub(crate) use flat::*;
pub(crate) use kernels::element_tensor_expr;
pub(crate) use kinds::*;
pub(crate) use record_layouts::{AuthoredRecordScope, record_layout};
pub(crate) use rtcalls::*;

/// Wrap an operand expression into the value union by its kind. Total
/// over the joinable kinds (union, Int, Rat, Bool): the union-lane
/// kind rules only admit those beside a `CodeValue` operand, so the
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

/// Wrap an operand expression into the node-record family by its
/// kind. Total over the family-joinable kinds (node, the dual Code,
/// the union, Int, Rat, Bool): the node-lane kind rules fold exactly
/// those into the family, mirroring the VM's heterogeneous `CValue`
/// sequences (a walk's `[made_code, node.args[1]]` list mixes Code
/// and node values).
pub(crate) fn to_node(expr: Expr, kind: &ValueKind) -> Expr {
    match kind {
        // Both family carriers are non-copy: an operand may render as
        // an owning register or a borrow block, so the fold clones
        // through method-call auto-deref (one spelling, both shapes).
        ValueKind::Node => Expr::Raw(format!("({}).clone()", render_expr(&expr))),
        ValueKind::ExprCode => Expr::Raw(format!(
            "emath_rt::code_tree::NodeValue::Code(Box::new(({}).clone()))",
            render_expr(&expr)
        )),
        // A list of family-joinable elements (the walk's all-made
        // argument lists, `[made, made]`) folds into the family's
        // dynamic sequence elementwise.
        ValueKind::Vector(element)
            if matches!(
                **element,
                ValueKind::Node
                    | ValueKind::ExprCode
                    | ValueKind::CodeValue
                    | ValueKind::I64
                    | ValueKind::Rational
                    | ValueKind::Bool
            ) =>
        {
            let folded = to_node(Expr::Raw(String::from("item")), element);
            Expr::Raw(format!(
                "emath_rt::code_tree::NodeValue::Sequence(({}).into_iter().map(|item| {}).collect::<Vec<_>>())",
                render_expr(&expr),
                render_expr(&folded)
            ))
        }
        ValueKind::CodeValue => Expr::Raw(format!(
            "emath_rt::code_tree::NodeValue::Scalar({})",
            render_expr(&expr)
        )),
        ValueKind::I64 => Expr::Raw(format!(
            "emath_rt::code_tree::NodeValue::Scalar(emath_rt::code::CodeValue::Int({}))",
            render_expr(&expr)
        )),
        ValueKind::Rational => Expr::Raw(format!(
            "emath_rt::code_tree::NodeValue::Scalar(emath_rt::code::CodeValue::Rat({}))",
            render_expr(&expr)
        )),
        ValueKind::Bool => Expr::Raw(format!(
            "emath_rt::code_tree::NodeValue::Scalar(emath_rt::code::CodeValue::Bool({}))",
            render_expr(&expr)
        )),
        other => unreachable!(
            "the node-lane kind rules admit only node/Code/union/Int/Rat/Bool into the family, found {other:?}"
        ),
    }
}

/// Render a shared tree as a Rust value expression (the static-tree
/// embed): every distillable shape renders as its constructing
/// expression, deterministic and allocation-minimal.
pub(crate) fn tree_expr(tree: &emath_rt::code_tree::CodeTree) -> String {
    use emath_rt::code_tree::{CodeTree, TreeBinary, TreeUnary};
    fn binary(op: TreeBinary) -> String {
        format!("emath_rt::code_tree::TreeBinary::{op:?}")
    }
    fn unary(op: TreeUnary) -> String {
        format!("emath_rt::code_tree::TreeUnary::{op:?}")
    }
    fn code_value(value: &emath_rt::code::CodeValue) -> String {
        match value {
            emath_rt::code::CodeValue::Int(n) => {
                format!("emath_rt::code::CodeValue::Int({n}i64)")
            }
            emath_rt::code::CodeValue::Rat((num, den)) => {
                format!("emath_rt::code::CodeValue::Rat(({num}i128, {den}i128))")
            }
            emath_rt::code::CodeValue::Bool(b) => {
                format!("emath_rt::code::CodeValue::Bool({b})")
            }
        }
    }
    match tree {
        CodeTree::Literal(value) => {
            format!(
                "emath_rt::code_tree::CodeTree::Literal({})",
                code_value(value)
            )
        }
        CodeTree::Path(segments) => format!(
            "emath_rt::code_tree::CodeTree::Path(vec![{}])",
            segments
                .iter()
                .map(|segment| format!("String::from({segment:?})"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        CodeTree::Call { function, args } => format!(
            "emath_rt::code_tree::CodeTree::Call {{ function: Box::new({}), args: vec![{}] }}",
            tree_expr(function),
            args.iter().map(tree_expr).collect::<Vec<_>>().join(", ")
        ),
        CodeTree::Binary { op, left, right } => format!(
            "emath_rt::code_tree::CodeTree::Binary {{ op: {}, left: Box::new({}), right: Box::new({}) }}",
            binary(*op),
            tree_expr(left),
            tree_expr(right)
        ),
        CodeTree::Unary { op, value } => format!(
            "emath_rt::code_tree::CodeTree::Unary {{ op: {}, value: Box::new({}) }}",
            unary(*op),
            tree_expr(value)
        ),
        CodeTree::Tuple(items) => format!(
            "emath_rt::code_tree::CodeTree::Tuple(vec![{}])",
            items.iter().map(tree_expr).collect::<Vec<_>>().join(", ")
        ),
        CodeTree::If {
            condition,
            then_value,
            else_value,
        } => format!(
            "emath_rt::code_tree::CodeTree::If {{ condition: Box::new({}), then_value: Box::new({}), else_value: Box::new({}) }}",
            tree_expr(condition),
            tree_expr(then_value),
            tree_expr(else_value)
        ),
    }
}

pub(crate) fn op_expr(
    op: &EmirOp,
    program: &EmirProgram,
    names: &[String],
    states: &[String],
    input_kinds: &InputKinds,
    kinds: &[ValueKind],
) -> Result<Expr, BackendError> {
    match op {
        EmirOp::CallFrame {
            body,
            inputs,
            state,
            declared,
        } => literal_frame_expr(body, inputs, state, program, kinds, declared),
        EmirOp::CallSelf { inputs, result } => {
            // The recursive target (`__self` at entries, `__frame_self`
            // in inlined frames) declares owned parameters for
            // non-copy carriers and `&dyn Fn` for closures. Arguments
            // follow: records and vectors re-materialize owned
            // (clone), closure carriers rest as `Rc<dyn Fn>` registers
            // so one borrow coerces to the `&dyn Fn` parameter.

            let args = inputs
                .iter()
                .enumerate()
                .map(|(position, value)| {
                    let kind = kind_at(kinds, *value);
                    // A node value crossing into the recursive target's
                    // declared Code parameter (the input at this
                    // position) unwraps/rebuilds through the shared
                    // bridge - the VM's call arguments are codes, and
                    // the node lane wraps them as Code nodes.
                    let param_kind = names
                        .get(position)
                        .and_then(|name| input_kinds.get(name).cloned());
                    if let Some(target) = &param_kind {
                        if let Some(converted) =
                            numeric_boundary_value(&operand(program, *value), &kind, target)
                        {
                            return render_expr(&converted);
                        }
                    }
                    if kind == ValueKind::Node && param_kind == Some(ValueKind::ExprCode) {
                        return format!(
                            "emath_rt::code_tree::node_as_code(&{})?",
                            render_expr(&operand(program, *value))
                        );
                    }
                    if matches!(kind, ValueKind::Closure { .. }) {
                        // Closure carriers are shared `Rc` handles:
                        // clone into the `Rc<dyn Fn>` parameter.
                        format!("{}.clone()", render_expr(&operand(program, *value)))
                    } else if kind.is_copy() {
                        render_expr(&operand(program, *value))
                    } else {
                        render_expr(&owned_operand(program, *value, kinds))
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            // The recursive wrapper's signature is the enclosing
            // body's inferred result carrier (`program_kind` over the
            // same context - exactly the kind the `__self` /
            // `__frame_self` wrapper emits), while this call site's
            // kind inference reads the authored output signature (the
            // i64 lane for an `Int` result, the element it names for
            // a sequence). The two disagree exactly when the body
            // joins machine-op results (ExactInt), so the call result
            // crosses through the shared numeric boundary - the same
            // law as call arguments: E-INT-002 by name on narrowing,
            // exact widening the other way, and sequence elements
            // obey the same check element-wise.
            let callee_kind = program_kind(program, names, states, input_kinds);
            let declared =
                if result.is_empty() { ValueKind::Other } else { ValueKind::from_signature(result) };
            let site_kind = if !matches!(declared, ValueKind::Other) {
                declared
            } else {
                inputs
                    .first()
                    .map_or(ValueKind::I64, |value| kind_at(kinds, *value))
            };
            let call = Expr::Raw(format!("__self({args})?"));
            let crossed = numeric_boundary_value(&call, &callee_kind, &site_kind)
                .map(|converted| render_expr(&converted))
                .unwrap_or_else(|| render_expr(&call));
            Ok(Expr::Raw(crossed))
        }
        EmirOp::CallSibling {
            name,
            inputs,
            declared,
            ..
        } => {
            // A sibling call that closes a recursion cycle: the
            // callee's body cannot inline (the cycle would nest
            // forever), so the call renders against the callee's
            // emitted entry fn - one named entry per runnable
            // function lives in the same crate. Arguments cross
            // through the callee's declared carriers (the same
            // frame-input law: numeric boundaries, closure handle
            // clones, owned non-copy carriers).
            let entry = escape_ident(name);
            let args = inputs
                .iter()
                .enumerate()
                .map(|(position, value)| {
                    let kind = kind_at(kinds, *value);
                    let target = declared
                        .get(position)
                        .map(|signature| ValueKind::from_signature(signature))
                        .filter(|kind| !matches!(kind, ValueKind::Other));
                    if let Some(target) = &target {
                        if let Some(converted) =
                            numeric_boundary_value(&operand(program, *value), &kind, target)
                        {
                            return render_expr(&converted);
                        }
                    }
                    if matches!(kind, ValueKind::Closure { .. }) {
                        format!("{}.clone()", render_expr(&operand(program, *value)))
                    } else if kind.is_copy() {
                        render_expr(&operand(program, *value))
                    } else {
                        render_expr(&owned_operand(program, *value, kinds))
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            Ok(Expr::Raw(format!("{entry}({args})?")))
        }
        EmirOp::SameDenseShape(..)
        | EmirOp::DenseValues(_)
        | EmirOp::DenseRepack { .. }
        | EmirOp::ToF64(_)
        | EmirOp::DenseLayout(_)
        | EmirOp::VectorSlice { .. }
        | EmirOp::VectorConcat(_)
        | EmirOp::ListConcat(_)
        | EmirOp::F64SortTotal(_) => op_collection_exprs(op, program, kinds),
        EmirOp::Branch { .. }
        | EmirOp::Iterate { .. }
        | EmirOp::Collect { .. }
        | EmirOp::Refuse(_)
        | EmirOp::RefuseValue(_) => authored_control_expr(op, program, kinds),
        EmirOp::ListCreate(elements) => {
            // A list with any node-family element joins the family's
            // dynamic sequence (the walk's mixed `[made, arg]`
            // argument lists); element values fold via `to_node`.
            if elements
                .iter()
                .any(|value| kind_at(kinds, *value) == ValueKind::Node)
            {
                let items = elements
                    .iter()
                    .map(|value| {
                        let kind = kind_at(kinds, *value);
                        render_expr(&to_node(operand(program, *value), &kind))
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                return Ok(Expr::Raw(format!(
                    "emath_rt::code_tree::node_list(vec![{items}])"
                )));
            }
            {
                let element_kind = numeric_element_kind(elements, kinds);
                Ok(Expr::Macro {
                    name: "vec".into(),
                    args: elements
                        .iter()
                        .map(|value| {
                            numeric_boundary_value(
                                &operand(program, *value),
                                &kind_at(kinds, *value),
                                &element_kind,
                            )
                            .unwrap_or_else(|| owned_operand(program, *value, kinds))
                        })
                        .collect(),
                })
            }
        }
        EmirOp::RecordField { record, field } => {
            // A field off a node value is the family's dynamic
            // projection - a typed refusal on a missing field, never
            // a silent default (the VM's `project_field` law).

            if kind_at(kinds, *record) == ValueKind::Node {
                let record_code = render_expr(&operand(program, *record));
                return Ok(Expr::Raw(format!("({record_code}).field({field:?})?")));
            }
            // The authored `p.length` sugar: a `length` projection
            // off a sequence-shaped receiver is the storage length,
            // the same Int the `length(x)` builtin computes (the
            // reference VM resolves it dynamically over CValue
            // sequences); the storage lane mirrors VectorLength's.
            if field == "length"
                && matches!(
                    kind_at(kinds, *record),
                    ValueKind::Vector(_) | ValueKind::Matrix(_) | ValueKind::Tensor
                )
            {
                let value_code = render_expr(&operand(program, *record));
                let storage = match kind_at(kinds, *record) {
                    ValueKind::Matrix(_) => format!("({value_code}).as_slice()"),
                    ValueKind::Tensor => format!("({value_code}).data"),
                    _ => value_code,
                };
                return Ok(Expr::Raw(format!("({storage}.len() as i64)")));
            }
            // A part projection off the Rational carrier reads the
            // `(i128, i128)` tuple (`numer` = `.0`, `denom` = `.1`) and
            // widens to the exact-integer carrier, mirroring the VM's
            // `CValue::Rat { num, den }` parts. Anything but
            // numer/denom is a typed refusal, never a silent default.
            if kind_at(kinds, *record) == ValueKind::Rational {
                let index = match field.as_str() {
                    "numer" => "0",
                    "denom" => "1",
                    _ => {
                        return Err(BackendError::UnsupportedType(format!(
                            "Rat part projection `{field}` is not numer/denom"
                        )));
                    }
                };
                return Ok(Expr::Raw(format!(
                    "emath_rt::ExactInt::from(({}).{index})",
                    render_expr(&operand(program, *record))
                )));
            }
            let value = Expr::Field {
                receiver: Box::new(operand(program, *record)),
                field: escape_ident(field),
            };
            if kind_of_op(op, kinds, names, states, input_kinds, None).is_copy() {
                Ok(value)
            } else {
                Ok(Expr::Raw(format!("&{}", render_expr(&value))))
            }
        }
        EmirOp::ToInt(value) => checked_integer_operand(program, *value, kinds),
        EmirOp::IntegerQuotient(left, right) => {
            let left_k = kind_at(kinds, *left);
            let right_k = kind_at(kinds, *right);
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
                    render_expr(&exact_int_operand(program, *left, kinds)),
                    render_expr(&exact_int_operand(program, *right, kinds))
                )));
            }
            Err(BackendError::UnsupportedType(
                "quotient requires two Int operands".into(),
            ))
        }
        EmirOp::SameBits(left, right) => Ok(Expr::Raw(format!(
            "({}).to_bits() == ({}).to_bits()",
            render_expr(&typed_operand(program, *left, ValueKind::F64, kinds)),
            render_expr(&typed_operand(program, *right, ValueKind::F64, kinds))
        ))),
        EmirOp::ConstF64(bits) => Ok(Expr::F64(*bits)),
        EmirOp::ConstI64(value) => Ok(Expr::Int(*value)),
        EmirOp::ConstExactInt(digits) => Ok(Expr::Raw(format!(
            "emath_rt::ExactInt::parse(\"{digits}\").expect(\"const-exact-int digits\")"
        ))),
        EmirOp::ExactIntCall { name, args } => Ok(exact_int_call_expr(name, args, program, kinds)?),
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
        | EmirOp::RecordCreate { .. } => {
            op_data_exprs(op, program, names, states, input_kinds, kinds)
        }
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
        | EmirOp::IsFinite(..) => op_arith_exprs(op, program, kinds),
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
        | EmirOp::TensorSlice { .. } => op_collection_exprs(op, program, kinds),
        EmirOp::Fold { .. }
        | EmirOp::OptionSome(..)
        | EmirOp::OptionNone
        | EmirOp::OptionIsSome(..)
        | EmirOp::OptionUnwrapOr(..)
        | EmirOp::ResultOk(..)
        | EmirOp::ResultErr(..)
        | EmirOp::ResultIsOk(..)
        | EmirOp::ResultUnwrapOr(..)
        | EmirOp::ResultErrorOf(..) => {
            op_flow_exprs(op, program, names, states, input_kinds, kinds)
        }
        EmirOp::ApplyCapability {
            capability,
            class,
            args,
        } => capability_artifact_expr(capability, *class, args, program, kinds),
        EmirOp::ToF64Vector(sequence) => {
            if kind_at(kinds, *sequence) != ValueKind::Vector(Box::new(ValueKind::F64)) {
                return Err(BackendError::UnsupportedType(
                    "vector packing requires Float64 elements".into(),
                ));
            }
            Ok(owned_operand(program, *sequence, kinds))
        }
        EmirOp::CallProgram {
            program: callable,
            inputs,
        }
        | EmirOp::CallScalarProgram {
            program: callable,
            inputs,
        }
        | EmirOp::CallRealProgram {
            program: callable,
            inputs,
        }
        | EmirOp::TryCallRealProgram {
            program: callable,
            inputs,
        } => {
            if kind_at(kinds, *callable) != ValueKind::Program
                || kind_at(kinds, *inputs) != ValueKind::Vector(Box::new(ValueKind::F64))
            {
                return Err(BackendError::UnsupportedType(
                    "numeric call requires Program and Vector<Float64>".into(),
                ));
            }
            let call = format!(
                "({})(&{})",
                render_expr(&operand(program, *callable)),
                render_expr(&operand(program, *inputs))
            );
            Ok(match op {
                EmirOp::CallProgram { .. } => map_runtime_result(format!(
                    "{call}.and_then(emath_rt::NumericProgramResult::into_vector)"
                )),
                EmirOp::CallScalarProgram { .. } => map_runtime_result(format!(
                    "{call}.and_then(emath_rt::NumericProgramResult::into_scalar)"
                )),
                EmirOp::CallRealProgram { .. } => map_runtime_result(format!(
                    "{call}.and_then(emath_rt::NumericProgramResult::into_real)"
                )),
                _ => Expr::Raw(format!(
                    "{call}.and_then(emath_rt::NumericProgramResult::into_real)"
                )),
            })
        }
        EmirOp::CallValue {
            program: callee_value,
            inputs,
        } => {
            // Typed indirect call on a closure carrier. The callee's
            // declared signature decides arity and borrowing: copy
            // arguments render owned, reference carriers borrow, and
            // the closure's own Result propagates with `?`. A curried
            // callee consumes its arguments one stage at a time -
            // `(f)(a)?` yields the next callable (boxed), which the
            // following stage calls.

            let ValueKind::Closure { .. } = kind_at(kinds, *callee_value) else {
                return Err(BackendError::UnsupportedType(
                    "closure call requires a callee with a declared signature".into(),
                ));
            };
            let callee = render_expr(&operand(program, *callee_value));
            let mut call = format!("({callee})");
            let mut remaining: &[EmirValue] = inputs;
            let mut kind = kind_at(kinds, *callee_value);
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
                        if let Some(converted) = numeric_boundary_value(
                            &operand(program, *value),
                            &kind_at(kinds, *value),
                            param,
                        ) {
                            let converted = render_expr(&converted);
                            return if param.is_copy() {
                                converted
                            } else {
                                format!("&({converted})")
                            };
                        }
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
                        let already_borrowed =
                            program.ops.get(value.0 as usize).is_some_and(|(op, _)| {
                                matches!(op, EmirOp::LoadInput(_) | EmirOp::LoadState(_))
                            });
                        if already_borrowed {
                            expression
                        } else {
                            format!("&{expression}")
                        }
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
        EmirOp::ProgramLiteral {
            body,
            captures,
            vector_input,
            signature,
        } => {
            if body.state_count != 0 {
                return Err(BackendError::UnsupportedType(
                    "program literal must be closed over state".into(),
                ));
            }
            let parameters = (0..body.input_count)
                .map(|index| format!("__program_arg_{index}"))
                .collect::<Vec<_>>();
            let explicit = usize::from(body.input_count)
                .checked_sub(captures.len())
                .ok_or_else(|| {
                    BackendError::UnsupportedType("program captures exceed input count".into())
                })?;

            if signature.is_empty() {
                if *vector_input && explicit != 1 {
                    return Err(BackendError::UnsupportedType(
                        "vector program requires exactly one explicit input".into(),
                    ));
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
                        let kind = kind_at(kinds, argument);
                        let value = render_expr(&owned_operand(program, argument, kinds));
                        capture_bindings
                            .push_str(&format!("let __program_capture_{index} = {value}; "));
                        let borrow = if kind.is_copy() { "" } else { "&" };
                        bindings
                            .push_str(&format!("let {name} = {borrow}__program_capture_{index}; "));
                        kind
                    };
                    inputs.insert(name.clone(), kind);
                }
                let _reference = ReferenceScope::enter();
                let body_code = render_expr(&value_expr(body, &parameters, &[], &inputs)?);
                let result = match program_kind(body, &parameters, &[], &inputs) {
                    ValueKind::F64 => {
                        format!("emath_rt::NumericProgramResult::Scalar({body_code})")
                    }
                    ValueKind::I64 => {
                        format!("emath_rt::NumericProgramResult::Integer({body_code})")
                    }
                    ValueKind::Never => body_code,
                    ValueKind::Vector(element) if *element == ValueKind::F64 => {
                        format!("emath_rt::NumericProgramResult::Vector({body_code})")
                    }
                    _ => {
                        return Err(BackendError::UnsupportedType(
                            "numeric program must return Float64, Int, or Vector<Float64>".into(),
                        ));
                    }
                };
                let arity_guard = if *vector_input {
                    String::new()
                } else {
                    format!(
                        "if __program_inputs.len() != {explicit} {{ return Err(String::from(\"call-program: argument count mismatch\")); }} "
                    )
                };
                Ok(Expr::Raw(format!(
                    "{{ {capture_bindings} let __program: std::sync::Arc<dyn Fn(&[f64]) -> Result<emath_rt::NumericProgramResult, String>> = std::sync::Arc::new(move |__program_inputs: &[f64]| {{ {arity_guard} {bindings} Ok({result}) }}); __program }}"
                )))
            } else {
                // Typed literal: the authored parameter domain fixes the
                // callable ABI. One explicit parameter, owned captures
                // moved into the closure, body ops propagate with `?`,
                // and the inferred result kind types the return.
                if *vector_input {
                    return Err(BackendError::UnsupportedType(
                        "vector program carrier is the numeric lane only".into(),
                    ));
                }
                if explicit != 1 {
                    return Err(BackendError::UnsupportedType(
                        "typed program literal takes exactly one domain parameter".into(),
                    ));
                }
                let param_kind = ValueKind::from_signature(signature);
                if matches!(param_kind, ValueKind::Other) {
                    return Err(BackendError::UnsupportedType(format!(
                        "program literal parameter domain {signature} has no native carrier"
                    )));
                }
                let param_ty = {
                    if matches!(param_kind, ValueKind::Closure { .. }) {
                        // A closure parameter's rust type already
                        // carries the `&dyn Fn` layer; borrowing again
                        // would double it.
                        crate::rust_ir::render::render_ty(&param_kind.rust_ty()?)
                    } else {
                        let ty = crate::rust_ir::render::render_ty(&param_kind.rust_ty()?);
                        if param_kind.is_copy() {
                            ty
                        } else {
                            format!("&{ty}")
                        }
                    }
                };
                let mut inputs = InputKinds::new();
                let mut capture_bindings = String::new();
                let mut bindings = String::new();
                inputs.insert(parameters[0].clone(), param_kind);
                for (index, name) in parameters.iter().enumerate().skip(1) {
                    let argument = captures[index - 1];
                    let kind = kind_at(kinds, argument);
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
                    let value = render_expr(&owned_operand(program, argument, kinds));
                    capture_bindings
                        .push_str(&format!("let __program_capture_{index} = {value}; "));
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
                            .map_or_else(|| "?".into(), |(op, _)| op.name().to_string());
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
            }
        }
        EmirOp::CodeLiteral {
            body,
            param,
            free,
            carrier,
            tree,
            deps,
        } => {
            if param.is_none() {
                // The compiled EXPRESSION template (bead
                // emath-expression-quotes-324y0): the quoted
                // expression lowers once over the value union - every
                // scalar op of the body renders as a dynamic rt kernel
                // call implementing the VM's exact carrier rules. All
                // free names are runtime inputs (the hygiene law
                // keeps them open); the factory computes the
                // union-valued expression directly - no parameter, no
                // inner closure. The body-kind check is the named
                // gate: anything the union lane cannot compute
                // (structured values, floats, non-scalar ops, a
                // closed body with no free name to carry the union)
                // refuses here.
                //
                // The dual representation (bead
                // emath-shared-tree-view-make-bp8nu): the SAME
                // lowering emits the distilled tree and the
                // capture-time dependency snapshot beside the
                // factory, so `quote.view`/`quote.make` operate on
                // exactly the program this factory computes - the two
                // halves cannot drift because neither is written
                // separately.
                let mut inputs = InputKinds::new();
                for name in free {
                    inputs.insert(name.clone(), ValueKind::CodeValue);
                }
                let body_kind = program_kind(body, free, &[], &inputs);
                // A closed scalar body (a bare literal, e.g. `quote(0)`
                // in an authored transformation rule) still computes
                // the union - the atom wraps into it; the free-name
                // arithmetic path stays CodeValue-typed throughout.
                // Everything else the union lane cannot compute
                // refuses named.
                let body_value = if body_kind == ValueKind::CodeValue {
                    value_expr(body, free, &[], &inputs)?
                } else if matches!(
                    body_kind,
                    ValueKind::I64 | ValueKind::Rational | ValueKind::Bool
                ) {
                    to_code_value(value_expr(body, free, &[], &inputs)?, &body_kind)
                } else {
                    return Err(BackendError::UnsupportedType(
                    "quote expression template body must compute the scalar union (Int/Rat/Bool arithmetic, comparisons, and boolean combinators over its free names)".into(),
                ));
                };
                let _reference = ReferenceScope::enter();
                let body_code = render_expr(&body_value);
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
                let tree_code = tree_expr(
                    tree.as_ref()
                        .expect("union lane carries the distilled tree"),
                );
                let deps_code = if deps.is_empty() {
                    "std::collections::BTreeMap::new()".to_string()
                } else {
                    format!(
                        "vec![{}].into_iter().collect::<std::collections::BTreeMap<String, u64>>()",
                        deps.iter()
                            .map(|(name, stamp)| format!("(String::from({name:?}), {stamp}u64)"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                };
                return Ok(Expr::Raw(format!(
                    "{{ emath_rt::code::open_expr(vec![{free_list}], {tree_code}, Some(std::rc::Rc::new(move |__code_free: &[emath_rt::code::CodeValue]| -> Result<emath_rt::code::CodeValue, String> {{ {bindings} Ok({body_code}) }})), {deps_code}) }}"
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
            let carrier_ty = crate::rust_ir::render::render_ty(&carrier_kind.rust_ty()?);
            let unary_ty =
                format!("std::rc::Rc<dyn Fn({carrier_ty}) -> Result<{carrier_ty}, String>>");
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
        EmirOp::CodeSubstitute {
            code,
            reference,
            value,
        } => {
            // Partial application of one open name. The reference
            // resolved statically at lowering. A FUNCTION template
            // demands the declared carrier (the carrier check is
            // named at emission; the generic instantiation would not
            // compile a mismatch anyway). An EXPRESSION template
            // accepts any scalar - the substitute-time carrier fact -
            // converted into the union.

            match kind_at(kinds, *code) {
                ValueKind::Code(carrier) => {
                    let value_kind = kind_at(kinds, *value);
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
                    let value_kind = kind_at(kinds, *value);
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
                        "({}).substitute({:?}, {})",
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
            // typed boundary) - through the compiled factory when the
            // tree came from a static template literal, through the
            // shared scalar tree evaluator when it was made from
            // node records. Stamped dependencies verify against the
            // module table first (the `stale_dependency` refusal).

            match kind_at(kinds, *code) {
                ValueKind::Code(_) => Ok(Expr::Raw(format!(
                    "emath_rt::code::evaluate(&{})?",
                    render_expr(&operand(program, *code))
                ))),
                ValueKind::ExprCode => Ok(Expr::Raw(format!(
                    "({}).evaluate(&__EMATH_MODULE_TABLE)?",
                    render_expr(&operand(program, *code))
                ))),
                _ => Err(BackendError::UnsupportedType(
                    "code-evaluate requires a Code value".into(),
                )),
            }
        }
        EmirOp::CodeView { code } => {
            // quote.view: the shared tree walked into node records -
            // minted Scope witnesses included - over the module
            // table's Global layouts. A Code value views its own
            // tree; a Fragment node value views its term.

            match kind_at(kinds, *code) {
            ValueKind::ExprCode => Ok(Expr::Raw(format!(
                "({}).view(&__EMATH_MODULE_TABLE)",
                render_expr(&operand(program, *code))
            ))),
            ValueKind::Node => Ok(Expr::Raw(format!(
                "({}).view(&__EMATH_MODULE_TABLE)?",
                render_expr(&operand(program, *code))
            ))),
            ValueKind::Code(_) => Err(BackendError::UnsupportedType(
                "quote.view is emitted for expression templates; a unary function template has no tree lane in this cut".into(),
            )),
            _ => Err(BackendError::UnsupportedType(
                "quote.view requires a Code or Fragment value".into(),
            )),
        }
        }
        EmirOp::CodeMake { node } => {
            // quote.make: rebuild a (possibly modified) node-record
            // tree back into the dual Code value; a Code passthrough
            // stays itself. The rebuilt open names and dependency
            // stamps follow the VM's quote_make laws.

            match kind_at(kinds, *node) {
                ValueKind::Node => Ok(Expr::Raw(format!(
                    "({}).make(&__EMATH_MODULE_TABLE)?",
                    render_expr(&operand(program, *node))
                ))),
                ValueKind::ExprCode => {
                    let value = to_node(operand(program, *node), &ValueKind::ExprCode);
                    Ok(Expr::Raw(format!(
                        "({}).make(&__EMATH_MODULE_TABLE)?",
                        render_expr(&value)
                    )))
                }
                _ => Err(BackendError::UnsupportedType(
                    "quote.make requires a node record or Code value".into(),
                )),
            }
        }
        EmirOp::CodeBody { code } => {
            // quote.body: the definition-table unfold - a Code naming
            // a module function becomes the Available/Opaque body
            // record over the embedded definition table (the VM's
            // quote_body laws; a transparent body outside the
            // distilled subset refuses by name).

            match kind_at(kinds, *code) {
            ValueKind::ExprCode => Ok(Expr::Raw(format!(
                "({}).body(&__EMATH_DEFINITIONS)?",
                render_expr(&operand(program, *code))
            ))),
            ValueKind::Code(_) => Err(BackendError::UnsupportedType(
                "quote.body is emitted for expression templates; a unary function template has no definition-table lane in this cut".into(),
            )),
            _ => Err(BackendError::UnsupportedType(
                "quote.body requires a Code value".into(),
            )),
        }
        }
        EmirOp::CodeOpen { package } => {
            // quote.open (the binder form's unwrap): a Fragment node
            // package validates its minted Scope witness and unwraps
            // its term; a plain Code opens to its tree with the
            // factory and snapshot dropped (the VM's
            // check_fragment_scope + open_fragment laws, bead
            // emath-quote-bind-open-consumer-6f86g).

            match kind_at(kinds, *package) {
            ValueKind::Node => Ok(Expr::Raw(format!(
                "({}).open()?",
                render_expr(&operand(program, *package))
            ))),
            ValueKind::ExprCode => Ok(Expr::Raw(format!(
                "({}).open()",
                render_expr(&operand(program, *package))
            ))),
            ValueKind::Code(_) => Err(BackendError::UnsupportedType(
                "quote.open is emitted for expression templates; a unary function template has no tree lane in this cut".into(),
            )),
            _ => Err(BackendError::UnsupportedType(
                "quote.open requires a Code or Fragment package value".into(),
            )),
        }
        }
        EmirOp::CodeBind { code } => {
            // quote.bind (the call form): the mint walk - the
            // identity over the distilled subset - with the
            // dependency snapshot re-stamped against the module
            // table (the VM's mint_binds + dependency_snapshot laws).
            // The binder FORM (a fresh-tokened function literal) is
            // outside the distilled subset and refuses at lowering.

            match kind_at(kinds, *code) {
            ValueKind::ExprCode => Ok(Expr::Raw(format!(
                "({}).bind(&__EMATH_MODULE_TABLE)",
                render_expr(&operand(program, *code))
            ))),
            ValueKind::Code(_) => Err(BackendError::UnsupportedType(
                "quote.bind is emitted for expression templates; a unary function template has no tree lane in this cut".into(),
            )),
            _ => Err(BackendError::UnsupportedType(
                "quote.bind requires a Code value".into(),
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
                    let mut inputs = names
                        .iter()
                        .zip(args)
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
                        let code =
                            render_expr(&value_expr(default, &names[..index], &[], &inputs)?);
                        let kind = program_kind(default, &names[..index], &[], &inputs);
                        source.push_str(&format!("let {} = {code}; ", names[index]));
                        inputs.insert(names[index].clone(), kind);
                    }
                    let body = render_expr(&value_expr(&cell.program, &names, &[], &inputs)?);
                    source.push_str(&body);
                    source.push_str(" }");
                    drop(_reference);
                    if rate_context() || fold_context() {
                        return Ok(map_runtime_result(format!(
                            "(|| -> Result<_, String> {{ Ok({source}) }})()"
                        )));
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
                    .map_or("", |(s, _)| s);
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
                    "emath_rt::{}({arguments})",
                    artifact.rust_function
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
            artifact.render(args, program, kinds).ok_or_else(|| {
                BackendError::UnsupportedType(format!(
                    "kernel {} refuses argument carriers {:?}",
                    artifact.kernel_id,
                    args.iter()
                        .map(|arg| kind_at(kinds, *arg))
                        .collect::<Vec<_>>()
                ))
            })
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
    let Ok(binding) = emath_exec_ir::native_kernel::verified_kernel_binding(capability) else {
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
    fn render(
        &self,
        args: &[EmirValue],
        program: &EmirProgram,
        kinds: &[ValueKind],
    ) -> Option<Expr> {
        match self.kind {
            KernelArtifactKind::DensePointIndex => {
                op_collections::dense_point_index_expr(args, program, kinds)
            }
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
    if inputs.len() != usize::from(body.input_count) || state.len() != usize::from(body.state_count)
    {
        return Err(BackendError::UnsupportedType(
            "frame argument count mismatch".into(),
        ));
    }
    let names = (0..inputs.len())
        .map(|index| format!("__frame_input_{index}"))
        .collect::<Vec<_>>();
    let states = (0..state.len())
        .map(|index| format!("__frame_state_{index}"))
        .collect::<Vec<_>>();
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
        // A node value whose frame kind resolved to the declared Code
        // carrier (the view's args/children wrap codes as Code nodes)
        // crosses through the shared bridge: unwrap or rebuild, never
        // a silent type change.
        if kind == ValueKind::ExprCode && kind_at(kinds, *value) == ValueKind::Node {
            code.push_str(&format!(
                "let {name} = &emath_rt::code_tree::node_as_code({})?; ",
                render_expr(&operand(outer, *value))
            ));
            frame_kinds.insert(name.clone(), kind);
            continue;
        }
        // A widened frame input (an Int actual into a Rat-declared
        // parameter) crosses through the same checked boundary the
        // call sites use; the binding owns the converted carrier.
        let actual = kind_at(kinds, *value);
        if actual != kind {
            if let Some(converted) = numeric_boundary_value(&operand(outer, *value), &actual, &kind)
            {
                code.push_str(&format!("let {name} = {}; ", render_expr(&converted)));
                frame_kinds.insert(name.clone(), kind);
                continue;
            }
        }
        if kind.is_copy() {
            code.push_str(&format!(
                "let {name} = {}; ",
                render_expr(&operand(outer, *value))
            ));
        } else {
            // A borrowed frame binding carries its carrier in the
            // binding itself: a degenerate operand (an empty `[]`
            // literal) has no self-evident element type to infer, so
            // the declared frame kind names it.
            let ty = crate::rust_ir::render::render_ty(&kind.borrowed_rust_ty()?);
            code.push_str(&format!(
                "let {name}: &{ty} = &{}; ",
                render_expr(&operand(outer, *value))
            ));
        }
        frame_kinds.insert(name.clone(), kind);
    }
    for (name, value) in states.iter().zip(state) {
        let kind = kind_at(kinds, *value);
        let borrow = if kind.is_copy() { "" } else { "&" };
        code.push_str(&format!(
            "let {name} = {borrow}{}; ",
            render_expr(&operand(outer, *value))
        ));
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
                call_args.push(format!("<{ty} as Clone>::clone(&*{name})"));
            }
        }
        let result_kind = program_kind(body, &names, &states, &frame_kinds);
        let result_ty = if result_kind == ValueKind::Never {
            // A frame whose every path refuses never produces a value
            // (the Rust never type is experimental): the unit carrier
            // names the unreachable result, and the body diverges.
            "()".to_string()
        } else {
            match result_kind.rust_ty() {
                Ok(ty) => crate::rust_ir::render::render_ty(&ty),
                Err(_) => "impl core::fmt::Debug".to_string(),
            }
        };
        let mut body_code = render_expr(&value_expr(body, &names, &states, &frame_kinds)?)
            .replace("__self(", "__frame_self(");
        // A frame tail that resolves to a borrowed register (a
        // LoadInput binding) must return the owned carrier the
        // recursive fn's signature promises; `owned_value` handles
        // both owned and borrowed register shapes. A pure-refusal
        // tail has no value to own - it diverges.
        if !result_kind.is_copy()
            && result_kind != ValueKind::Never
            && !matches!(result_kind, ValueKind::Closure { .. })
        {
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
    code.push_str(&render_expr(&value_expr(
        body,
        &names,
        &states,
        &frame_kinds,
    )?));
    code.push_str(" }");
    Ok(Expr::Raw(code))
}
