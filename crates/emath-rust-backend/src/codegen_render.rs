use std::collections::{BTreeMap, HashMap};

use crate::rust_ir::ast::{BinOp, Block, Expr, Stmt, Ty, UnOp, escape_ident};
use crate::rust_ir::render::render_expr;
use emath_exec_ir::optimize::{is_total, operand_registers};
use emath_exec_ir::{CodeSubRef, EmirOp, EmirProgram, EmirSliceAxis, EmirValue, FoldCombine};

use crate::BackendError;
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
mod kinds;
mod record_layouts;
mod rtcalls;

use carrier::{expect_result_carrier, op_self_index};
pub(crate) use flat::*;
pub(crate) use kinds::*;
pub(crate) use record_layouts::{
    AuthoredRecordScope, RecordTemplate, RecordTemplateScope, infer_instantiation, record_ident,
    record_layout, record_template, stored_field_signature, take_instantiations,
    touch_record_kinds,
};
pub(crate) use rtcalls::*;

/// Replace `pat` in rendered Rust only where it starts an identifier
/// and `next` accepts the following character. Scoped declaration keys
/// (`name__module`) contain `__`, so a plain `str::replace` of a
/// generated-name prefix would rewrite user function names too.
pub(crate) fn replace_ident_prefix(
    src: &str,
    pat: &str,
    rep: &str,
    next: impl Fn(Option<char>) -> bool,
) -> String {
    let mut out = String::with_capacity(src.len());
    let mut rest = src;
    let mut prev: Option<char> = None;
    while let Some(at) = rest.find(pat) {
        let before = rest[..at].chars().next_back().or(if at == 0 { prev } else { None });
        let boundary = !before.is_some_and(|c| c.is_alphanumeric() || c == '_');
        let after = rest[at + pat.len()..].chars().next();
        out.push_str(&rest[..at]);
        if boundary && next(after) {
            out.push_str(rep);
        } else {
            out.push_str(pat);
        }
        prev = pat.chars().next_back();
        rest = &rest[at + pat.len()..];
    }
    out.push_str(rest);
    out
}

/// Wrap a supported scalar in the code union. Inferred operands can
/// carry other kinds, so an unsupported crossing is an emission refusal.
pub(crate) fn to_code_value(expr: Expr, kind: &ValueKind) -> Result<Expr, BackendError> {
    Ok(match kind {
        ValueKind::CodeValue => Expr::Raw(format!("({}).clone()", render_expr(&expr))),
        ValueKind::Evaluated => Expr::Raw(format!("({}).to_scalar()?", render_expr(&expr))),
        ValueKind::Cell => Expr::Raw(format!("({}).scalar()?", render_expr(&expr))),
        ValueKind::Node => Expr::Raw(format!("({}).to_scalar()?", render_expr(&expr))),
        ValueKind::I64 => Expr::Raw(format!(
            "emath_rt::code::CodeValue::Int({})",
            render_expr(&expr)
        )),
        ValueKind::Rational => Expr::Raw(format!(
            "emath_rt::code::CodeValue::Rat(({}).clone())",
            render_expr(&expr)
        )),
        ValueKind::Bool => Expr::Raw(format!(
            "emath_rt::code::CodeValue::Bool({})",
            render_expr(&expr)
        )),
        // The union's Int is i64; the VM's arbitrary-precision Int
        // narrows checked (E-INT-002), never truncated - the same law
        // as `to_node`.
        ValueKind::ExactInt => Expr::Raw(format!(
            "emath_rt::code::CodeValue::Int({})",
            render_expr(&kinds::coerce_to_ty(expr, ValueKind::ExactInt, &Ty::I64))
        )),
        other => {
            return Err(BackendError::UnsupportedType(format!(
                "code union conversion cannot represent {other:?}; supported carriers are Int/Rat/Bool"
            )));
        }
    })
}

/// Whether a value of this kind crosses into and out of the reference
/// engine an artifact links (`artifact_bridge`). An authored record
/// crosses when every field does; the constructor crate then emits the
/// bridge impls for its struct. A self-referencing layout stops at the
/// depth bound and does not cross.
pub(crate) fn engine_crosses(kind: &ValueKind) -> bool {
    fn crosses(kind: &ValueKind, depth: usize) -> bool {
        match kind {
            ValueKind::I64
            | ValueKind::ExactInt
            | ValueKind::Rational
            | ValueKind::Bool
            | ValueKind::F64
            | ValueKind::Text
            | ValueKind::ExprCode
            | ValueKind::Node => true,
            ValueKind::Vector(element) => crosses(element, depth),
            ValueKind::Record(name) if depth < 8 => record_layout(name).is_some_and(|layout| {
                layout
                    .iter()
                    .all(|(_, signature)| crosses(&ValueKind::from_signature(signature), depth + 1))
            }),
            _ => false,
        }
    }
    crosses(kind, 0)
}

/// The node-record family is closed under sequences: a family-joinable
/// element, or a nested sequence of one, folds inductively (the walk's
/// lists of all-made argument lists are `sequence(sequence(Code))`).
pub(crate) fn node_family(kind: &ValueKind) -> bool {
    match kind {
        ValueKind::Node
        | ValueKind::ExprCode
        | ValueKind::CodeValue
        | ValueKind::I64
        | ValueKind::ExactInt
        | ValueKind::Rational
        | ValueKind::Bool
        | ValueKind::Text => true,
        ValueKind::Vector(element) => node_family(element),
        _ => false,
    }
}

fn carries_exact_int(kind: &ValueKind) -> bool {
    match kind {
        ValueKind::ExactInt => true,
        ValueKind::Vector(element) => carries_exact_int(element),
        _ => false,
    }
}

/// Wrap an operand expression into the node-record family by its
/// kind. Total over the family-joinable kinds (node, the dual Code,
/// the union, Int, Rat, Bool, and sequences of those, nested
/// inductively). Unsupported crossings refuse during emission rather
/// than assuming kind inference proved membership. This mirrors the
/// supported subset of the VM's heterogeneous `CValue` sequences (a
/// walk's `[made_code, node.args[1]]` list mixes Code and node
/// values; its argument-list lists nest one level deeper).
pub(crate) fn to_node(expr: Expr, kind: &ValueKind) -> Result<Expr, BackendError> {
    Ok(match kind {
        // Both family carriers are non-copy: an operand may render as
        // an owning register or a borrow block, so the fold clones
        // through method-call auto-deref (one spelling, both shapes).
        ValueKind::Node => Expr::Raw(format!("({}).clone()", render_expr(&expr))),
        ValueKind::Text => Expr::Raw(format!(
            "emath_rt::code_tree::NodeValue::Text(({}).to_string())",
            render_expr(&expr)
        )),
        ValueKind::ExprCode => Expr::Raw(format!(
            "emath_rt::code_tree::NodeValue::Code(Box::new(({}).clone()))",
            render_expr(&expr)
        )),
        // A list of family-joinable elements (the walk's all-made
        // argument lists, `[made, made]`) folds into the family's
        // dynamic sequence elementwise; a nested family sequence folds
        // the same way, one map level per nesting.
        // An exact-Int element narrows with `?`, which cannot escape a
        // `map` closure, so that fold collects through a Result.
        ValueKind::Vector(element) if node_family(element) => {
            let folded = to_node(Expr::Raw(String::from("item")), element)?;
            if carries_exact_int(element) {
                return Ok(Expr::Raw(format!(
                    "emath_rt::code_tree::NodeValue::Sequence(({}).iter().cloned().map(|item| Ok::<_, String>({})).collect::<Result<Vec<_>, String>>()?)",
                    render_expr(&expr),
                    render_expr(&folded)
                )));
            }
            Expr::Raw(format!(
                "emath_rt::code_tree::NodeValue::Sequence(({}).iter().cloned().map(|item| {}).collect::<Vec<_>>())",
                render_expr(&expr),
                render_expr(&folded)
            ))
        }
        ValueKind::CodeValue => Expr::Raw(format!(
            "emath_rt::code_tree::NodeValue::Scalar(({}).clone())",
            render_expr(&expr)
        )),
        ValueKind::I64 => Expr::Raw(format!(
            "emath_rt::code_tree::NodeValue::Scalar(emath_rt::code::CodeValue::Int({}))",
            render_expr(&expr)
        )),
        // The code-tree scalar is i64; the VM's arbitrary-precision Int
        // narrows checked (E-INT-002), never truncated.
        ValueKind::ExactInt => Expr::Raw(format!(
            "emath_rt::code_tree::NodeValue::Scalar(emath_rt::code::CodeValue::Int({}))",
            render_expr(&kinds::coerce_to_ty(expr, ValueKind::ExactInt, &Ty::I64))
        )),
        ValueKind::Rational => Expr::Raw(format!(
            "emath_rt::code_tree::NodeValue::Scalar(emath_rt::code::CodeValue::Rat(({}).clone()))",
            render_expr(&expr)
        )),
        ValueKind::Bool => Expr::Raw(format!(
            "emath_rt::code_tree::NodeValue::Scalar(emath_rt::code::CodeValue::Bool({}))",
            render_expr(&expr)
        )),
        other => {
            return Err(BackendError::UnsupportedType(format!(
                "code node conversion cannot represent {other:?}; supported carriers are node/Code/union/Int/Rat/Bool and their sequences"
            )));
        }
    })
}

/// Render a shared tree as a Rust value expression (the static-tree
/// embed): every distillable shape renders as its constructing
/// expression, deterministic and allocation-minimal.
pub(crate) fn tree_expr(tree: &emath_rt::code_tree::CodeTree) -> String {
    use emath_rt::code_tree::{CodeTree, TreeBinary, TreeUnary};
    fn integer(value: &emath_rt::ExactInt) -> String {
        match value {
            emath_rt::ExactInt::Small(value) => format!("emath_rt::ExactInt::Small({value}i128)"),
            emath_rt::ExactInt::Big { neg, limbs } => {
                format!("emath_rt::ExactInt::Big {{ neg: {neg}, limbs: vec!{limbs:?} }}")
            }
        }
    }
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
            emath_rt::code::CodeValue::Rat(emath_rt::ExactRatio(num, den)) => {
                format!("emath_rt::code::CodeValue::Rat(emath_rt::ExactRatio({}, {}))", integer(num), integer(den))
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
        CodeTree::Closure {
            param,
            domain,
            body,
        } => {
            let domain = match domain {
                emath_rt::code_tree::TreeDomain::Scalar(name) => format!(
                    "emath_rt::code_tree::TreeDomain::Scalar(String::from({name:?}))"
                ),
                emath_rt::code_tree::TreeDomain::Sequence(name) => format!(
                    "emath_rt::code_tree::TreeDomain::Sequence(String::from({name:?}))"
                ),
                emath_rt::code_tree::TreeDomain::Unknown => {
                    String::from("emath_rt::code_tree::TreeDomain::Unknown")
                }
            };
            format!(
                "emath_rt::code_tree::CodeTree::Closure {{ param: String::from({param:?}), domain: {domain}, body: Box::new({}) }}",
                tree_expr(body)
            )
        }
        CodeTree::Recur { name, ty, body } => format!(
            "emath_rt::code_tree::CodeTree::Recur {{ name: String::from({name:?}), ty: Box::new({}), body: Box::new({}) }}",
            tree_expr(ty),
            tree_expr(body)
        ),
        CodeTree::Tuple(items) => format!(
            "emath_rt::code_tree::CodeTree::Tuple(vec![{}])",
            items.iter().map(tree_expr).collect::<Vec<_>>().join(", ")
        ),
        CodeTree::Index { container, indices } => format!(
            "emath_rt::code_tree::CodeTree::Index {{ container: Box::new({}), indices: vec![{}] }}",
            tree_expr(container),
            indices.iter().map(tree_expr).collect::<Vec<_>>().join(", ")
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
        EmirOp::BufferCreate { size, fill } => Ok(Expr::Raw(format!(
            "emath_rt::buffer::Buffer::new({}, {})?",
            render_expr(&typed_operand(program, *size, ValueKind::ExactInt, kinds)),
            cell_value(operand(program, *fill), &kind_at(kinds, *fill))?,
        ))),
        EmirOp::BufferWrite { buffer, index, value } => {
            let source = operand(program, *buffer);
            let source_kind = kind_at(kinds, *buffer);
            let source = if source_kind == ValueKind::Cell {
                cell_into(&render_expr(&source), &ValueKind::Buffer)
                    .ok_or_else(|| BackendError::UnsupportedType("buffer write carrier".into()))?
            } else if source_kind == ValueKind::Buffer {
                render_expr(&source)
            } else {
                return Err(BackendError::UnsupportedType("buffer_set expects a buffer".into()));
            };
            Ok(Expr::Raw(format!(
                "({source}).write({}, {})?",
                render_expr(&typed_operand(program, *index, ValueKind::ExactInt, kinds)),
                cell_value(operand(program, *value), &kind_at(kinds, *value))?,
            )))
        }
        EmirOp::VectorIndex { vector, index }
            if matches!(kind_at(kinds, *vector), ValueKind::Buffer | ValueKind::Cell) =>
        {
            let method = if kind_at(kinds, *vector) == ValueKind::Buffer { "read" } else { "index" };
            Ok(Expr::Raw(format!(
                "({}).{method}({})?",
                render_expr(&operand(program, *vector)),
                render_expr(&typed_operand(program, *index, ValueKind::ExactInt, kinds)),
            )))
        }
        EmirOp::VectorLength(value)
            if matches!(kind_at(kinds, *value), ValueKind::Buffer | ValueKind::Cell) =>
        {
            let is_buffer = kind_at(kinds, *value) == ValueKind::Buffer;
            let source = render_expr(&operand(program, *value));
            Ok(Expr::Raw(if is_buffer {
                format!("({source}).len()?")
            } else {
                format!("({source}).field(\"length\")?.get::<i64>()?")
            }))
        }
        EmirOp::CallFrame {
            body,
            inputs,
            state,
            declared,
            bindings,
            callee,
        } => {
            let _bindings = TypeBindingScope::enter_bound(bindings);
            if let Some(callee) = callee
                && state.is_empty()
                && let Some(call) = entry_frame_call(callee, body, inputs, program, kinds, declared)
            {
                return Ok(call);
            }
            let owns_self = crate::frame_owns_self(callee.as_deref(), declared);
            literal_frame_expr(body, inputs, state, program, kinds, declared, owns_self)
        }
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
                .map(|(position, value)| -> Result<String, BackendError> {
                    let kind = kind_at(kinds, *value);
                    // A node value crossing into the recursive target's
                    // declared Code parameter (the input at this
                    // position) unwraps/rebuilds through the shared
                    // bridge - the VM's call arguments are codes, and
                    // the node lane wraps them as Code nodes.
                    let param_kind = kinds::self_wrapper_param(position).or_else(|| {
                        names
                            .get(position)
                            .and_then(|name| input_kinds.get(name).cloned())
                    });
                    if let Some(target) = &param_kind {
                        if let Some(converted) = numeric_boundary_value(
                            &operand(program, *value),
                            &kind,
                            target,
                            "call argument",
                        ) {
                            return Ok(render_expr(&converted));
                        }
                    }
                    if kind == ValueKind::Node
                        && let Some(converted) = param_kind.as_ref().and_then(|target| {
                            node_into(&render_expr(&operand(program, *value)), target)
                        })
                    {
                        return Ok(converted);
                    }
                    if param_kind == Some(ValueKind::Node) && kind != ValueKind::Node {
                        return Ok(render_expr(&to_node(operand(program, *value), &kind)?));
                    }
                    Ok(if matches!(kind, ValueKind::Closure { .. }) {
                        // Closure carriers are shared `Rc` handles:
                        // clone into the `Rc<dyn Fn>` parameter.
                        format!("{}.clone()", render_expr(&operand(program, *value)))
                    } else if kind.is_copy() {
                        render_expr(&operand(program, *value))
                    } else {
                        render_expr(&owned_operand(program, *value, kinds))
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            if let Some((params, unchanged)) = kinds::tail_call_slot(op) {
                // Every new argument is computed before any parameter
                // is rebound, so arguments read the old values.
                let mut code = String::from("{ ");
                let moved: Vec<usize> = (0..args.len())
                    .filter(|position| !unchanged.get(*position).copied().unwrap_or(false))
                    .collect();
                for position in &moved {
                    code.push_str(&format!("let __tail_{position} = {}; ", args[*position]));
                }
                for position in &moved {
                    code.push_str(&format!("{} = __tail_{position}; ", params[*position]));
                }
                code.push_str("continue }");
                return Ok(Expr::Raw(code));
            }
            let args = args.join(", ");
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
            // obey the same check element-wise. Nested in a branch,
            // the local inference cannot see the enclosing join, so
            // the wrapper scope (set where the wrapper's type is
            // decided) names the carrier the call actually returns.
            let callee_kind = self_wrapper_kind()
                .unwrap_or_else(|| program_kind(program, names, states, input_kinds));
            let declared =
                if result.is_empty() { ValueKind::Other } else { ValueKind::from_signature(&bound_signature(result)) };
            let site_kind = op_self_index(program, op)
                .map(|index| kind_at(kinds, EmirValue(index)))
                .unwrap_or_else(|| {
                    if !matches!(declared, ValueKind::Other) {
                        declared
                    } else {
                        inputs
                            .first()
                            .map_or(ValueKind::I64, |value| kind_at(kinds, *value))
                    }
                });
            let call = Expr::Raw(format!("__self({args})?"));
            let crossed = numeric_boundary_value(&call, &callee_kind, &site_kind, "call result")
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
            let entry = internal_entry_name(name);
            let args = inputs
                .iter()
                .enumerate()
                .map(|(position, value)| {
                    let kind = kind_at(kinds, *value);
                    let target = sibling_entry_param(name, position).or_else(|| declared
                        .get(position)
                        .map(|signature| ValueKind::from_signature(signature))
                        .filter(|kind| !matches!(kind, ValueKind::Other)));
                    if let Some(target) = &target {
                        if let Some(converted) = numeric_boundary_value(
                            &operand(program, *value),
                            &kind,
                            target,
                            "call argument",
                        ) {
                            return render_expr(&converted);
                        }
                    }
                    // A node value crossing into the callee's declared
                    // Code (or sequence-of-Code) parameter rebuilds
                    // through the shared bridge - the same node-as-code
                    // law as the recursive `__self` call lane.
                    if kind == ValueKind::Node
                        && let Some(converted) = target.as_ref().and_then(|target| {
                            node_into(&render_expr(&operand(program, *value)), target)
                        })
                    {
                        return converted;
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
        EmirOp::ListConcat(_)
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
                        Ok(render_expr(&to_node(operand(program, *value), &kind)?))
                    })
                    .collect::<Result<Vec<_>, BackendError>>()?
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
                                "vector element",
                            )
                            .unwrap_or_else(|| owned_operand(program, *value, kinds))
                        })
                        .collect(),
                })
            }
        }
        EmirOp::RecordField { record, field } => {
            if kind_at(kinds, *record) == ValueKind::Buffer && field == "length" {
                return Ok(Expr::Raw(format!("({}).len()?", render_expr(&operand(program, *record)))));
            }
            if kind_at(kinds, *record) == ValueKind::Cell {
                return Ok(Expr::Raw(format!("({}).field({field:?})?", render_expr(&operand(program, *record)))));
            }
            // A field off a node value is the family's dynamic
            // projection - a typed refusal on a missing field, never
            // a silent default (the VM's `project_field` law). The
            // code-evaluation union projects through its node view
            // first: the union enum has no fields of its own (bead
            // emath-d09ve).

            if matches!(
                kind_at(kinds, *record),
                ValueKind::Node | ValueKind::Evaluated
            ) {
                let record_code = render_expr(&operand(program, *record));
                let receiver = if kind_at(kinds, *record) == ValueKind::Evaluated {
                    format!("({record_code}).to_node()?")
                } else {
                    record_code
                };
                return Ok(Expr::Raw(format!("({receiver}).field({field:?})?")));
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
            // A tuple-position projection (`_0`, `_1`, ...) off a
            // sequence carrier: checked positional indexing - the
            // VM's tuple field law (out of range refuses by name,
            // never a silent default). A node-family receiver keeps
            // the dynamic `field` arm above.
            if let Some(index) = field
                .strip_prefix('_')
                .and_then(|text| text.parse::<usize>().ok())
            {
                if let ValueKind::Vector(_) = kind_at(kinds, *record) {
                    let value_code = render_expr(&operand(program, *record));
                    return Ok(Expr::Raw(format!(
                        "(usize::try_from({index}i64).ok().and_then(|index| ({value_code}).get(index)).cloned()).ok_or_else(|| \"invalid_index: sequence index out of range\".to_string())?"
                    )));
                }
            }
            // A part projection off the Rational carrier reads the
            // exact parts (`numer` = `.0`, `denom` = `.1`), mirroring the VM's
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
                    "(({}).{index}).clone()",
                    render_expr(&operand(program, *record))
                )));
            }
            // A scalar, callable, or code carrier has no fields:
            // `t.field` over one is a lowering that took an
            // application for a projection, and it must refuse here -
            // never emit Rust that cannot compile behind a runnable
            // claim.
            let record_kind = kind_at(kinds, *record);
            if matches!(
                record_kind,
                ValueKind::I64
                    | ValueKind::F64
                    | ValueKind::Bool
                    | ValueKind::ExactInt
                    | ValueKind::Text
                    | ValueKind::Closure { .. }
                    | ValueKind::ExprCode
                    | ValueKind::Code(_)
                    | ValueKind::CodeValue
            ) {
                return Err(BackendError::UnsupportedType(format!(
                    "field `{field}` projected off a {record_kind:?} carrier, which has no fields"
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
        EmirOp::SameBits(left, right) => Ok(Expr::Raw(format!(
            "({}).to_bits() == ({}).to_bits()",
            render_expr(&typed_operand(program, *left, ValueKind::F64, kinds)),
            render_expr(&typed_operand(program, *right, ValueKind::F64, kinds))
        ))),
        EmirOp::ConstF64(bits) => Ok(Expr::F64(*bits)),
        EmirOp::ConstI64(value) if exact_twin_pass() => {
            Ok(Expr::Raw(format!("emath_rt::ExactInt::from({value}i64)")))
        }
        EmirOp::ConstI64(value) => Ok(Expr::Int(*value)),
        EmirOp::ConstExactInt(digits) => Ok(Expr::Raw(format!(
            "emath_rt::ExactInt::parse(\"{digits}\").expect(\"const-exact-int digits\")"
        ))),
        EmirOp::ExactIntCall { name, args } => Ok(exact_int_call_expr(name, args, program, kinds)?),
        EmirOp::ConstText(value) => Ok(Expr::Str(value.clone())),
        EmirOp::ConstBool(..)
        | EmirOp::LoadInput(..)
        | EmirOp::FormatText { .. }
        | EmirOp::RecordCreate { .. } => {
            op_data_exprs(op, program, names, input_kinds, kinds)
        }
        EmirOp::F64Add(..)
        | EmirOp::F64Sub(..)
        | EmirOp::F64Mul(..)
        | EmirOp::F64Div(..)
        | EmirOp::Neg(..)
        | EmirOp::Lt(..)
        | EmirOp::Le(..)
        | EmirOp::Gt(..)
        | EmirOp::Ge(..)
        | EmirOp::Eq(..)
        | EmirOp::Ne(..)
        | EmirOp::And(..)
        | EmirOp::Or(..)
        | EmirOp::Not(..)
        | EmirOp::IsFinite(..) => op_arith_exprs(op, program, kinds),
        EmirOp::VectorLength(..)
        | EmirOp::TextTrim(_)
        | EmirOp::TextLength(_)
        | EmirOp::IndexText(_)
        | EmirOp::TextByte(..)
        | EmirOp::FormatScientific(..)
        | EmirOp::ParseF64(_)
        | EmirOp::VectorIndex { .. }
        | EmirOp::TensorSlice { .. } => op_collection_exprs(op, program, kinds),
        EmirOp::Fold { .. }
        | EmirOp::ResultIsOk(..)
        | EmirOp::ResultUnwrapOr(..)
         => {
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

            if kind_at(kinds, *callee_value) == ValueKind::Evaluated {
                let args = inputs.iter().map(|value|
                    to_node(operand(program, *value), &kind_at(kinds, *value))
                        .map(|value| render_expr(&value))
                ).collect::<Result<Vec<_>, _>>()?.join(", ");
                return Ok(Expr::Raw(format!(
                    "({}).apply(&[{args}], &__EMATH_MODULE_TABLE)?",
                    render_expr(&operand(program, *callee_value)),
                )));
            }
            let ValueKind::Closure { .. } = kind_at(kinds, *callee_value) else {
                return Err(BackendError::UnsupportedType(format!(
                    "closure call requires a callee with a declared signature, found {:?} from {}",
                    kind_at(kinds, *callee_value),
                    program
                        .ops
                        .get(callee_value.0 as usize)
                        .map_or("an input", |(op, _)| op.name())
                )));
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
                    .map(|(value, param)| -> Result<String, BackendError> {
                        let expression = render_expr(&operand(program, *value));
                        // A scalar crossing into a union parameter (an
                        // applied quoted function) wraps into the union.
                        let value_kind = kind_at(kinds, *value);
                        if *param == ValueKind::CodeValue
                            && matches!(
                                value_kind,
                                ValueKind::I64 | ValueKind::Rational | ValueKind::Bool
                            )
                        {
                            return Ok(render_expr(&to_code_value(operand(program, *value), &value_kind)?));
                        }
                        // A viewed child (a node) crossing into a Code
                        // parameter: the VM's elements are codes.
                        if value_kind == ValueKind::Node
                            && let Some(converted) = node_into(&expression, param)
                        {
                            return Ok(format!("&({converted})"));
                        }
                        // Code or a scalar into a dynamic (node) binder.
                        if *param == ValueKind::Node
                            && value_kind != ValueKind::Node
                            && node_family(&value_kind)
                        {
                            let lifted = to_node(operand(program, *value), &value_kind)?;
                            return Ok(format!("&({})", render_expr(&lifted)));
                        }
                        if let Some(converted) = numeric_boundary_value(
                            &operand(program, *value),
                            &kind_at(kinds, *value),
                            param,
                            "call argument",
                        ) {
                            let converted = render_expr(&converted);
                            return Ok(if param.is_copy()
                                || matches!(param, ValueKind::Closure { .. })
                            {
                                converted
                            } else {
                                format!("&({converted})")
                            });
                        }
                        if param.is_copy() {
                            return Ok(expression);
                        }
                        // One reference layer exactly for data carriers:
                        // a LoadInput / LoadState register already
                        // rests as a borrowed binding, every other
                        // producer is owned (`&&T -> &T` never coerces
                        // at call arguments). Closure carriers are
                        // shared `Rc` handles and clone into the
                        // `Rc<dyn Fn>` parameter.
                        if matches!(param, ValueKind::Closure { .. }) {
                            return Ok(format!("{expression}.clone()"));
                        }
                        let already_borrowed =
                            program.ops.get(value.0 as usize).is_some_and(|(op, _)| {
                                matches!(op, EmirOp::LoadInput(_))
                            });
                        Ok(if already_borrowed {
                            expression
                        } else {
                            format!("&{expression}")
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
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
                    "{{ {capture_bindings} let __program: std::sync::Arc<dyn Fn(&[f64]) -> Result<emath_rt::NumericProgramResult, String>> = std::sync::Arc::new(move |__program_inputs: &[f64]| {{ let __native_call = emath_rt::execution::Frame::enter()?; {arity_guard} {bindings} Ok({result}) }}); __program }}"
                )))
            } else {
                // Typed literal: call-site inference retains the native
                // representation of its authored domain. Owned captures
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
                let param_kind = op_self_index(program, op)
                    .and_then(|index| match kinds.get(index as usize) {
                        Some(ValueKind::Closure { params, .. }) => params.first().cloned(),
                        _ => None,
                    })
                    .unwrap_or_else(|| ValueKind::from_signature(&bound_signature(signature)));
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
                        let ty = crate::rust_ir::render::render_ty(&param_kind.rust_ty().map_err(|error| {
                            BackendError::UnsupportedType(format!(
                                "literal domain {signature} binds as {param_kind:?}: {error}"
                            ))
                        })?);
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
                let depth = SharedCaptureScope::depth();
                let mut shared = vec![None; parameters.len()];
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
                    if kind.is_copy() {
                        let value = render_expr(&owned_operand(program, argument, kinds));
                        capture_bindings
                            .push_str(&format!("let __program_capture_{index} = {value}; "));
                        bindings.push_str(&format!("let {name} = __program_capture_{index}; "));
                    } else {
                        // Non-copy captures rest in a shared `Rc`: a
                        // literal nested in a body (a curried fold step
                        // rebuilt per element) re-shares the enclosing
                        // capture instead of copying the sequence per
                        // rebuild. Depth-suffixed names keep a nested
                        // literal's bindings from shadowing the
                        // enclosing captures they read.
                        let holder = format!("__program_shared_{depth}_{index}");
                        let value = match SharedCaptureScope::source(program, argument) {
                            Some(source) => format!("std::rc::Rc::clone(&{source})"),
                            None => format!(
                                "std::rc::Rc::new({})",
                                render_expr(&owned_operand(program, argument, kinds))
                            ),
                        };
                        capture_bindings.push_str(&format!("let {holder} = {value}; "));
                        bindings.push_str(&format!("let {name} = &*{holder}; "));
                        shared[index] = Some(holder);
                    }
                    inputs.insert(name.clone(), kind);
                }
                let _reference = ReferenceScope::enter();
                let _shared = SharedCaptureScope::enter(body, shared);
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
                    "{{ {capture_bindings} std::rc::Rc::new(move |{}: {param_ty}| -> Result<{result_ty}, String> {{ let __native_call = emath_rt::execution::Frame::enter()?; {bindings} {tail} }}) }}",
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
                // The compiled EXPRESSION template: the quoted
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
                // The dual representation: the SAME
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
                let compiled = if body_kind == ValueKind::CodeValue {
                    value_expr(body, free, &[], &inputs)
                } else if matches!(
                    body_kind,
                    ValueKind::I64 | ValueKind::Rational | ValueKind::Bool
                ) {
                    value_expr(body, free, &[], &inputs)
                        .and_then(|value| to_code_value(value, &body_kind))
                } else {
                    Err(BackendError::UnsupportedType(
                    "quote expression template body must compute the scalar union (Int/Rat/Bool arithmetic, comparisons, and boolean combinators over its free names)".into(),
                ))
                };
                // A template the union lane cannot compile (a call
                // shape like `quote(fold(x, y, z))` matched by a
                // structural walk, whose free names are not scalars)
                // carries its tree alone, as made code does:
                // evaluation runs the shared tree evaluator, which
                // refuses what it cannot compute.
                let body_value = match compiled {
                    Ok(value) => Some(value),
                    Err(_) if tree.is_some() => None,
                    Err(error) => return Err(error),
                };
                let _reference = ReferenceScope::enter();
                let factory = match body_value {
                    Some(body_value) => {
                        let body_code = render_expr(&body_value);
                        let bindings = free
                            .iter()
                            .enumerate()
                            .map(|(index, name)| format!("let {name} = __code_free[{index}].clone(); "))
                            .collect::<String>();
                        format!(
                            "Some(std::rc::Rc::new(move |__code_free: &[emath_rt::code::CodeValue]| -> Result<emath_rt::code::CodeValue, String> {{ {bindings} Ok({body_code}) }}))"
                        )
                    }
                    None => String::from("None"),
                };
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
                    "{{ emath_rt::code::open_expr(vec![{free_list}], {tree_code}, {factory}, {deps_code}) }}"
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
            if kinds::template_uses_tree(body, &param, free, carrier)
                && let Some(tree) = tree
            {
                let free = free.iter().map(|name| format!("String::from({name:?})"))
                    .collect::<Vec<_>>().join(", ");
                let deps = deps.iter().map(|(name, stamp)| format!("(String::from({name:?}), {stamp}u64)"))
                    .collect::<Vec<_>>().join(", ");
                return Ok(Expr::Raw(format!(
                    "emath_rt::code::open_expr(vec![{free}], {}, None, vec![{deps}].into_iter().collect())",
                    tree_expr(tree)
                )));
            }
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
                .map(|(index, name)| format!("let {name} = __code_free[{index}].clone(); "))
                .collect::<String>();
            // Free values survive every invocation of the compiled template.
            let mut forwarded = vec![param.clone()];
            forwarded.extend(free.iter().map(|name| format!("{name}.clone()")));
            let forwarded = forwarded.join(", ");
            let free_list = free
                .iter()
                .map(|name| format!("String::from({name:?})"))
                .collect::<Vec<_>>()
                .join(", ");
            // The dual representation: the compiled factory for
            // evaluation, the distilled tree for the view seam (the
            // same dual-rep the union lane carries). A template
            // outside the distilled subset (float carriers) carries
            // the factory alone - `quote.view` on it refuses by name
            // at runtime, the honest boundary, instead of the
            // emission refusing the whole function.
            let open_call = match tree {
                Some(tree) => format!(
                    "emath_rt::code::open_with_tree::<{carrier_ty}>(vec![{free_list}], {}, std::rc::Rc::new(move |__code_free: &[{carrier_ty}]| -> Result<{unary_ty}, String> {{ \
             {bindings} let __code_unary: {unary_ty} = std::rc::Rc::new(move |{param}: {carrier_ty}| __code_body({forwarded})); \
             Ok(__code_unary) }}))",
                    tree_expr(tree)
                ),
                None => format!(
                    "emath_rt::code::open::<{carrier_ty}>(vec![{free_list}], std::rc::Rc::new(move |__code_free: &[{carrier_ty}]| -> Result<{unary_ty}, String> {{ \
             {bindings} let __code_unary: {unary_ty} = std::rc::Rc::new(move |{param}: {carrier_ty}| __code_body({forwarded})); \
             Ok(__code_unary) }}))",
                ),
            };
            Ok(Expr::Raw(format!(
                "{{ let __code_body = move |{closure_params}| -> Result<{carrier_ty}, String> {{ let __native_call = emath_rt::execution::Frame::enter()?; {tail} }}; {open_call} }}"
            )))
        }
        EmirOp::CodeSubstitute {
            code,
            reference,
            value,
        } => {
            // Partial application of one open name. The reference is
            // either resolved statically at lowering (baked into the
            // call) or a runtime node-family operand: the
            // minted-scope packaging spells binder names as tag
            // records whose type name IS the name, extracted through
            // the family's `reference_name` law (the VM's dynamic
            // reference forms). A FUNCTION template demands the
            // declared carrier (the carrier check is named at
            // emission; the generic instantiation would not compile a
            // mismatch anyway). An EXPRESSION template accepts any
            // scalar - the substitute-time carrier fact - converted
            // into the union.

            let reference_code = match reference {
                CodeSubRef::Static(name) => format!("{name:?}"),
                CodeSubRef::Dyn(register) => {
                    if kind_at(kinds, *register) != ValueKind::Node {
                        return Err(BackendError::UnsupportedType(
                            "code-substitute reference must be node-carried for a runtime name"
                                .into(),
                        ));
                    }
                    format!(
                        "({}).reference_name()?.as_str()",
                        render_expr(&operand(program, *register))
                    )
                }
            };

            match kind_at(kinds, *code) {
                ValueKind::Code(carrier) => {
                    let value_kind = kind_at(kinds, *value);
                    // A numeric value crosses into the template's
                    // carrier through the shared boundary (the VM's
                    // Int-to-Rat admission widens exactly; a Rat into
                    // an Int carrier narrows checked, E-INT-002).
                    if value_kind != *carrier
                        && let Some(crossed) = numeric_boundary_value(
                            &operand(program, *value),
                            &value_kind,
                            carrier.as_ref(),
                            "substitute value",
                        )
                    {
                        return Ok(Expr::Raw(format!(
                            "emath_rt::code::substitute(&{}, {}, {})",
                            render_expr(&operand(program, *code)),
                            reference_code,
                            render_expr(&crossed)
                        )));
                    }
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
                        "emath_rt::code::substitute(&{}, {}, ({}).clone())",
                        render_expr(&operand(program, *code)),
                        reference_code,
                        render_expr(&operand(program, *value))
                    )))
                }
                code_kind @ (ValueKind::ExprCode | ValueKind::Node) => {
                    let value_kind = kind_at(kinds, *value);
                    // A node holding code (a viewed child, a dynamic
                    // `Code` binder) substitutes into the code it
                    // wraps.
                    let target = if code_kind == ValueKind::Node {
                        format!("({}).to_code()?", render_expr(&operand(program, *code)))
                    } else {
                        render_expr(&operand(program, *code))
                    };
                    // A node-record value (a viewed subtree) or a
                    // Code value rides the STRUCTURAL lane - the
                    // VM's `value_to_expr` law: the replacement
                    // rebuilds into a tree and splices as a tree.
                    // A scalar keeps the compiled-constant lane.
                    match value_kind {
                        ValueKind::Node => Ok(Expr::Raw(format!(
                            "({target}).substitute_tree({}, ({}).make(&__EMATH_MODULE_TABLE)?)",
                            reference_code,
                            render_expr(&operand(program, *value))
                        ))),
                        ValueKind::ExprCode => Ok(Expr::Raw(format!(
                            "({target}).substitute_tree({}, {})",
                            reference_code,
                            render_expr(&operand(program, *value))
                        ))),
                        // A sequence of scalars becomes a list of
                        // literals (`value_to_expr` over a sequence).
                        ValueKind::Vector(ref element) if node_family(element) => {
                            let owned = Expr::Raw(format!(
                                "({}).clone()",
                                render_expr(&operand(program, *value))
                            ));
                            let node = to_node(owned, &value_kind)?;
                            Ok(Expr::Raw(format!(
                                "({target}).substitute_tree({}, ({}).make(&__EMATH_MODULE_TABLE)?)",
                                reference_code,
                                render_expr(&node)
                            )))
                        }
                        // An exact integer narrows checked onto the
                        // union's Int (`to_code_value`).
                        value_kind
                            if (union_promotable(&value_kind)
                                || value_kind == ValueKind::ExactInt)
                                && value_kind != ValueKind::CodeValue =>
                        {
                            let converted = to_code_value(operand(program, *value), &value_kind)?;
                            Ok(Expr::Raw(format!(
                                "({target}).substitute({}, {})",
                                reference_code,
                                render_expr(&converted)
                            )))
                        }
                        _ => {
                            let name = |kind: &ValueKind| match kind {
                                ValueKind::Rational => "Rat".to_string(),
                                ValueKind::I64 => "Int".to_string(),
                                ValueKind::Bool => "Bool".to_string(),
                                other => format!("{other:?}"),
                            };
                            Err(BackendError::UnsupportedType(format!(
                                "code-substitute into an expression template requires a scalar, node-record, or Code value, found {}",
                                name(&value_kind)
                            )))
                        }
                    }
                }
                _ => Err(BackendError::UnsupportedType(
                    "code-substitute requires a Code value".into(),
                )),
            }
        }
        EmirOp::CodeEvaluate { code, applied } => {
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

            let code_kind = kind_at(kinds, *code);
            let target = if code_kind == ValueKind::Node {
                format!("({}).to_code()?", render_expr(&operand(program, *code)))
            } else {
                render_expr(&operand(program, *code))
            };
            match code_kind {
                ValueKind::Code(carrier) if !carrier.is_copy() => {
                    let ty = crate::rust_ir::render::render_ty(&carrier.rust_ty()?);
                    Ok(Expr::Raw(format!(
                        "{{ let __closed = emath_rt::code::evaluate(&{target})?; let __typed: std::rc::Rc<dyn Fn(&{ty}) -> Result<{ty}, String>> = std::rc::Rc::new(move |__argument: &{ty}| __closed(__argument.clone())); __typed }}"
                    )))
                }
                ValueKind::Code(_) => Ok(Expr::Raw(format!(
                    "emath_rt::code::evaluate(&{})?",
                    render_expr(&operand(program, *code))
                ))),
                ValueKind::ExprCode | ValueKind::Node => match applied {
                    // The declared lane: the same closed callable,
                    // adapted to the declared arrow - scalar arguments
                    // wrap into the union, the union result projects
                    // checked onto the declared carrier (the typed
                    // boundary's refusal names the slot).
                    Some(emath_exec_ir::CodeApplication::Declared(signature)) => {
                        if !signature.starts_with("Fn<") {
                            let kind = ValueKind::from_signature(signature);
                            let value = format!(
                                "({target}).evaluate_node(&__EMATH_MODULE_TABLE)?"
                            );
                            return node_into(&value, &kind).map(Expr::Raw).ok_or_else(|| {
                                BackendError::UnsupportedType(format!(
                                    "an evaluated quoted value has no native projection onto `{signature}`"
                                ))
                            });
                        }
                        let Some(ValueKind::Closure { params, result }) =
                            declared_scalar_arrow(signature)
                        else {
                            let kind = ValueKind::from_signature(signature);
                            let converted = numeric_boundary_value(
                                &Expr::Raw("__evaluated".into()),
                                &ValueKind::Evaluated, &kind, "evaluated result",
                            ).ok_or_else(|| BackendError::UnsupportedType(format!(
                                "an evaluated quoted function has no adapter onto `{signature}`"
                            )))?;
                            return Ok(Expr::Raw(format!(
                                "{{ let __evaluated = ({target}).evaluate_value(&__EMATH_MODULE_TABLE)?; {} }}",
                                render_expr(&converted),
                            )));
                        };
                        let bindings = params
                            .iter()
                            .enumerate()
                            .map(|(index, kind)| {
                                Ok(format!(
                                    "__arg_{index}: {}{}",
                                    if kind.is_copy() { "" } else { "&" },
                                    crate::rust_ir::render::render_ty(&kind.rust_ty()?)
                                ))
                            })
                            .collect::<Result<Vec<_>, BackendError>>()?
                            .join(", ");
                        let union_args = params
                            .iter()
                            .enumerate()
                            .map(|(index, kind)| {
                                Ok(render_expr(&to_code_value(
                                    Expr::Raw(format!("__arg_{index}")),
                                    kind,
                                )?))
                            })
                            .collect::<Result<Vec<_>, BackendError>>()?
                            .join(", ");
                        let projected = numeric_boundary_value(
                            &Expr::Raw(String::from("__out")),
                            &ValueKind::CodeValue,
                            &result,
                            "evaluated result",
                        )
                        .map(|expr| render_expr(&expr))
                        .ok_or_else(|| {
                            BackendError::UnsupportedType(format!(
                                "an evaluated quoted function has no projection onto `{signature}`"
                            ))
                        })?;
                        let callable = callable_ty(&params, &result)?;
                        Ok(Expr::Raw(format!(
                            "{{ let __applied = ({}).evaluate_callable(&__EMATH_MODULE_TABLE)?; let __typed: std::rc::Rc<{callable}> = std::rc::Rc::new(move |{bindings}| {{ let __out = __applied(&[{union_args}])?; Ok({projected}) }}); __typed }}",
                            target
                        )))
                    }
                    // The applied lane: the rt closes the code once
                    // (the same unbound/stale refusals, at the same
                    // point) and hands back a slice callable; the
                    // shared `Rc<dyn Fn>` carrier spreads it over one
                    // union parameter per applied argument.
                    Some(emath_exec_ir::CodeApplication::Union(arity)) => {
                        let params = (0..*arity)
                            .map(|index| format!("__apply_{index}: &emath_rt::code_tree::NodeValue"))
                            .collect::<Vec<_>>()
                            .join(", ");
                        let names = (0..*arity)
                            .map(|index| format!("__apply_{index}.clone()"))
                            .collect::<Vec<_>>()
                            .join(", ");
                        let callable = callable_ty(
                            &vec![ValueKind::Node; usize::from(*arity)],
                            &ValueKind::Evaluated,
                        )?;
                        Ok(Expr::Raw(format!(
                            "{{ let __applied = ({}).evaluate_value(&__EMATH_MODULE_TABLE)?; let __applied: std::rc::Rc<{callable}> = std::rc::Rc::new(move |{params}| __applied.apply(&[{names}], &__EMATH_MODULE_TABLE)); __applied }}",
                            target
                        )))
                    }
                    None => Ok(Expr::Raw(format!(
                        "({}).evaluate(&__EMATH_MODULE_TABLE)?",
                        target
                    ))),
                },
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
            ValueKind::Code(_) => Ok(Expr::Raw(format!(
                "({}).view(&__EMATH_MODULE_TABLE)?",
                render_expr(&operand(program, *code))
            ))),
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
                // A Code passes through; a scalar (an evaluated
                // union, Int, Rat, or Bool) rebuilds as a closed
                // literal of its exact value - the VM's quote_make
                // over a value (a folded `5/1 + 2*3` is the literal
                // 11/1: rationals stay rational, never truncated).
                kind @ (ValueKind::ExprCode
                | ValueKind::CodeValue
                | ValueKind::I64
                | ValueKind::ExactInt
                | ValueKind::Rational
                | ValueKind::Bool) => {
                    let value = to_node(operand(program, *node), &kind)?;
                    Ok(Expr::Raw(format!(
                        "({}).make(&__EMATH_MODULE_TABLE)?",
                        render_expr(&value)
                    )))
                }
                _ => Err(BackendError::UnsupportedType(
                    "quote.make requires a node record, Code, or scalar value".into(),
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
            // A node value (the Call node's `callee` field, viewed):
            // a Code-carrying node unfolds through the definition
            // table; an operator tag record is the no-identity
            // Opaque record - the VM's `quote_body` law arm for arm
            // (`quote_body_over_node`).
            ValueKind::Node => Ok(Expr::Raw(format!(
                // The untyped first binding keeps the operand's own type
                // (owned, or borrowed as a closure parameter); the typed
                // second binding then deref-coerces either to one layer.
                "emath_rt::code_tree::quote_body_over_node({{ let __held = &({}); let __node: &emath_rt::code_tree::NodeValue = __held; __node }}, &__EMATH_DEFINITIONS)?",
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
            // check_fragment_scope + open_fragment laws).

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
        EmirOp::CodeIdentity { code } => {
            // quote.identity: the VM's canonical identity over the
            // distilled tree. A view's child is a node; it crosses into
            // Code through the call-marshal bridge first.
            let code_expr = render_expr(&operand(program, *code));
            match kind_at(kinds, *code) {
                ValueKind::ExprCode | ValueKind::Code(_) => {
                    Ok(Expr::Raw(format!("({code_expr}).identity()?")))
                }
                ValueKind::Node => Ok(Expr::Raw(format!("({code_expr}).to_code()?.identity()?"))),
                _ => Err(BackendError::UnsupportedType(
                    "quote.identity requires a Code value".into(),
                )),
            }
        }
        EmirOp::EngineCall {
            function,
            inputs,
            result,
        } => {
            // The artifact runs this function in the reference engine:
            // the crate links `emath-exec-ir` and embeds the module
            // source (see `constructor_crate`).
            let crosses = engine_crosses;
            const BRIDGE: &str = "emath_exec_ir::constructor_layer::artifact_bridge";
            let mut args = Vec::with_capacity(inputs.len());
            for (input, value) in inputs {
                let kind = kind_at(kinds, *value);
                if !crosses(&kind) {
                    return Err(BackendError::UnsupportedType(format!(
                        "`{function}` runs in the reference engine; its input `{input}` carries {kind:?}, which does not cross into the engine"
                    )));
                }
                args.push(format!(
                    "({input:?}, {BRIDGE}::ToCValue::to_cvalue(&{}))",
                    render_expr(&operand(program, *value))
                ));
            }
            let call = format!(
                "{BRIDGE}::run_entry(__EMATH_SOURCE, __EMATH_SOURCE_PATH, __EMATH_MODULE_ROOTS, __EMATH_MODULES, {function:?}, vec![{}], __EMATH_WORLDS)?",
                args.join(", ")
            );
            let kind = ValueKind::from_signature(&bound_signature(result));
            let refuse = || {
                BackendError::UnsupportedType(format!(
                    "`{function}` runs in the reference engine; its declared result `{result}` does not cross back"
                ))
            };
            let read = if crosses(&kind) {
                let ty = kind.rust_ty().map_err(|_| refuse())?;
                format!(
                    "{BRIDGE}::read::<{}>(&__answer)?",
                    crate::rust_ir::render::render_ty(&ty)
                )
            } else {
                return Err(refuse());
            };
            Ok(Expr::Raw(format!("{{ let __answer = {call}; {read} }}")))
        }
        EmirOp::CodeJoin { left, right } => {
            // Both operands cross into the node family (a quoted name,
            // a name tag, or an Int suffix); the runtime checks them.
            let mut sides = Vec::with_capacity(2);
            for value in [*left, *right] {
                let kind = kind_at(kinds, value);
                if !node_family(&kind) {
                    return Err(BackendError::UnsupportedType(format!(
                        "quote.join expects quoted identifiers, found {kind:?}"
                    )));
                }
                sides.push(render_expr(&to_node(operand(program, value), &kind)?));
            }
            Ok(Expr::Raw(format!(
                "emath_rt::code_tree::join_quoted(&({}), &({}), &__EMATH_MODULE_TABLE)?",
                sides[0], sides[1]
            )))
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
                    if fold_context() {
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
            let artifact = KERNEL_ARTIFACTS
                .iter()
                .find(|artifact| {
                    artifact.kernel_id == binding.kernel_id
                        && artifact.signature == binding.signature
                        && artifact.semantic_hash == binding.semantic_hash
                })
                .ok_or_else(|| BackendError::StaleArtifactBinding(capability.to_string()))?;
            artifact.render(args, program).ok_or_else(|| {
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
    CheckedAdd,
}



impl KernelArtifact {
    fn render(&self, args: &[EmirValue], program: &EmirProgram) -> Option<Expr> {
        match self.kind {
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
];

/// A named frame renders as a call to its callee's emitted entry when
/// the frame computes exactly what the entry computes: every frame input
/// carrier (the same `frame_input_kind` law the inline frame binds with)
/// equals the entry's parameter carrier, and the frame's inferred result
/// equals the carrier the entry returns. Then the inline body and the
/// entry are one computation over one set of kinds, and the site's
/// register kind is unchanged - emitting it once instead of at every call
/// site is what keeps nested sibling calls from multiplying the artifact.
/// A tree-lane callee is called through its non-resetting body fn (the
/// run's mint continues). Any disagreement (a call-site specialization,
/// a `CodeValue` body the entry projects, a diverging body) keeps the
/// inline frame.
fn entry_frame_call(
    callee: &str,
    body: &EmirProgram,
    inputs: &[EmirValue],
    outer: &EmirProgram,
    kinds: &[ValueKind],
    declared: &[String],
) -> Option<Expr> {
    let abi = entry_abi(callee)?;
    if abi.inputs.len() != inputs.len()
        || usize::from(body.input_count) != inputs.len()
        || body.state_count != 0
    {
        return None;
    }
    let names = (0..inputs.len())
        .map(|index| format!("__frame_input_{index}"))
        .collect::<Vec<_>>();
    let mut frame_kinds = InputKinds::new();
    for (index, (name, value)) in names.iter().zip(inputs).enumerate() {
        let kind = frame_input_kind(kind_at(kinds, *value), declared.get(index));
        if kind != abi.inputs[index] {
            return None;
        }
        frame_kinds.insert(name.clone(), kind);
    }
    let result = program_kind(body, &names, &[], &frame_kinds);
    if result != abi.result || result == ValueKind::Never || result.rust_ty().is_err() {
        return None;
    }
    let mut args = Vec::with_capacity(inputs.len());
    for (value, target) in inputs.iter().zip(&abi.inputs) {
        let actual = kind_at(kinds, *value);
        let source = operand(outer, *value);
        let arg = if actual == *target || kind_is_degenerate(&actual) {
            if matches!(actual, ValueKind::Closure { .. }) {
                format!("{}.clone()", render_expr(&source))
            } else if target.is_copy() {
                render_expr(&source)
            } else {
                render_expr(&owned_operand(outer, *value, kinds))
            }
        } else if actual == ValueKind::Node
            && let Some(converted) = node_into(&render_expr(&source), target)
        {
            converted
        } else {
            render_expr(&numeric_boundary_value(
                &source,
                &actual,
                target,
                "call argument",
            )?)
        };
        args.push(arg);
    }
    Some(Expr::Raw(format!(
        "crate::{}({})?",
        internal_entry_name(callee),
        args.join(", ")
    )))
}

/// A node-family value crossing into a declared code carrier (the
/// call-marshal bridge): `Code` unwraps or rebuilds through
/// `node_as_code`, and a declared sequence of code (`sequence(Code)`,
/// `sequence((Code, Code))`) maps the family sequence's items through
/// the same bridge, recursively - the VM's call arguments ARE codes and
/// sequences of them. `None` for any other target.
pub(crate) fn node_into(expression: &str, target: &ValueKind) -> Option<String> {
    match target {
        ValueKind::ExprCode => Some(format!("({expression}).to_code()?")),
        // A scalar node into a declared scalar: the checked projection
        // (a Rational refuses an Int parameter by name).
        ValueKind::I64 | ValueKind::Rational | ValueKind::Bool => {
            let projection = match target {
                ValueKind::I64 => "project_i64",
                ValueKind::Rational => "project_ratio",
                _ => "project_bool",
            };
            Some(format!(
                "emath_rt::code::{projection}(&({expression}).to_scalar()?, \"argument\")?"
            ))
        }
        // A family element of a family sequence (`sequence(T)` with T
        // bound to the node family) stays a node.
        ValueKind::Node => Some(format!("({expression}).clone()")),
        ValueKind::Vector(element) => {
            let inner = node_into("__item", element)?;
            let element_ty = crate::rust_ir::render::render_ty(&element.rust_ty().ok()?);
            Some(format!(
                "({expression}).items()?.into_iter().map(|__item| -> Result<{element_ty}, String> {{ Ok({inner}) }}).collect::<Result<Vec<{element_ty}>, String>>()?"
            ))
        }
        _ => None,
    }
}

/// Preserve a native value's concrete carrier while exposing immutable
/// structural projections. Nested buffer handles clone their identity.
fn cell_value(expression: Expr, kind: &ValueKind) -> Result<String, BackendError> {
    let source = render_expr(&expression);
    if *kind == ValueKind::Cell {
        return Ok(format!("({source}).clone()"));
    }
    let ty = crate::rust_ir::render::render_ty(&kind.rust_ty()?);
    let owned = if *kind == ValueKind::Text {
        format!("({source}).to_string()")
    } else {
        format!("<{ty} as Clone>::clone(&({source}))")
    };
    let metadata = match kind {
        ValueKind::Vector(element) => {
            let item = cell_value(Expr::Raw("__buffer_item".into()), element)?;
            format!(".with_items(__buffer_value.iter().map(|__buffer_item| {item}).collect())")
        }
        ValueKind::Record(name) => {
            let layout = record_layout(name)
                .ok_or_else(|| BackendError::UnsupportedType(format!("buffer record `{name}` has no layout")))?;
            let mut fields = Vec::new();
            for (name, signature) in layout {
                let field = Expr::Raw(format!("__buffer_value.{}", escape_ident(&name)));
                let value = cell_value(field, &ValueKind::from_signature(&signature))?;
                fields.push(format!("(String::from({name:?}), {value})"));
            }
            format!(".with_fields(vec![{}].into_iter().collect())", fields.join(", "))
        }
        _ => String::new(),
    };
    Ok(format!("{{ let __buffer_value: {ty} = {owned}; emath_rt::buffer::CellValue::new(__buffer_value.clone()){metadata} }}"))
}

pub(crate) fn cell_into(expression: &str, target: &ValueKind) -> Option<String> {
    if *target == ValueKind::Cell {
        return Some(format!("({expression}).clone()"));
    }
    if let ValueKind::Vector(element) = target {
        let item = cell_into("__buffer_item", element)?;
        return Some(format!("({expression}).sequence()?.into_iter().map(|__buffer_item| -> Result<_, String> {{ Ok({item}) }}).collect::<Result<Vec<_>, String>>()?"));
    }
    let ty = crate::rust_ir::render::render_ty(&target.rust_ty().ok()?);
    Some(format!("({expression}).get::<{ty}>()?"))
}

fn literal_frame_expr(
    body: &EmirProgram,
    inputs: &[EmirValue],
    state: &[EmirValue],
    outer: &EmirProgram,
    kinds: &[ValueKind],
    declared: &[String],
    owns_self: bool,
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
    let arg_kinds = inputs.iter().map(|value| kind_at(kinds, *value)).collect::<Vec<_>>();
    let _types = TypeBindingScope::enter(&arg_kinds, declared);
    let declared = &instantiate_declared(&arg_kinds, declared);
    let mut refined = InputKinds::new();
    for (index, (name, value)) in names.iter().zip(inputs).enumerate() {
        let actual = kind_at(kinds, *value);
        // The scalar union cannot bind a sequence (or any non-scalar)
        // parameter: the inlined body would index or measure a scalar
        // and the crate would not compile. Refusing here lets an
        // expression template keep its tree without a compiled factory.
        if actual == ValueKind::CodeValue
            && let Some(signature) = declared.get(index)
            && !matches!(
                ValueKind::from_signature(signature),
                ValueKind::CodeValue
                    | ValueKind::I64
                    | ValueKind::ExactInt
                    | ValueKind::Rational
                    | ValueKind::Bool
                    | ValueKind::Other
            )
        {
            return Err(BackendError::UnsupportedType(format!(
                "a scalar union value cannot bind a `{signature}` parameter"
            )));
        }
        refined.insert(name.clone(), frame_input_kind(actual, declared.get(index)));
    }
    for (name, value) in states.iter().zip(state) {
        refined.insert(name.clone(), kind_at(kinds, *value));
    }
    refine_recursive_frame_kinds(body, &names, &states, &mut refined);
    let mut frame_kinds = InputKinds::new();
    let mut code = String::from("{ ");
    for (name, value) in names.iter().zip(inputs) {
        let kind = refined.get(name).cloned().unwrap_or(ValueKind::Other);
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
        if kind_at(kinds, *value) == ValueKind::Node
            && let Some(converted) = node_into(&render_expr(&operand(outer, *value)), &kind)
        {
            let borrow = if kind.is_copy() { "" } else { "&" };
            code.push_str(&format!("let {name} = {borrow}{converted}; "));
            frame_kinds.insert(name.clone(), kind);
            continue;
        }
        // A widened frame input (an Int actual into a Rat-declared
        // parameter) crosses through the same checked boundary the
        // call sites use; the binding owns the converted carrier.
        let actual = kind_at(kinds, *value);
        if actual != kind {
            if kind == ValueKind::Node && kinds::kind_is_degenerate(&actual)
                && matches!(actual, ValueKind::Vector(_))
            {
                code.push_str(&format!(
                    "let {name} = &emath_rt::code_tree::NodeValue::Sequence({}); ",
                    render_expr(&operand(outer, *value))
                ));
                frame_kinds.insert(name.clone(), kind);
                continue;
            }
            if let Some(converted) = numeric_boundary_value(
                &operand(outer, *value),
                &actual,
                &kind,
                "frame input",
            ) {
                let borrow = if kind.is_copy() { "" } else { "&" };
                code.push_str(&format!("let {name} = {borrow}{}; ", render_expr(&converted)));
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
    if owns_self && contains_call_self(body) {
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
        // The frame fn IS the recursive wrapper for its body: the
        // scope names its decided carrier so nested `CallSelf` sites
        // cross from what `__frame_self` actually returns.
        let param_names: Vec<String> = names.iter().chain(states.iter()).cloned().collect();
        let param_kinds = param_names
            .iter()
            .map(|name| frame_kinds.get(name).cloned().unwrap_or(ValueKind::Other))
            .collect();
        let _wrapper = SelfWrapperScope::enter(result_kind.clone(), param_kinds);
        let _tail = TailLoopScope::enter(body, param_names, states.len());
        let mut body_code = replace_ident_prefix(
            &render_expr(&value_expr(body, &names, &states, &frame_kinds)?),
            "__self(",
            "__frame_self(",
            |_| true,
        );
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
            "#[allow(unused_mut, unreachable_code)] fn __frame_self({}) -> Result<{result_ty}, String> {{ let __native_call = emath_rt::execution::Frame::enter()?; loop {{ return Ok({body_code}); }} }} __frame_self({})? ",
            params
                .iter()
                .map(|param| format!("mut {param}"))
                .collect::<Vec<_>>()
                .join(", "),
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

fn comparison(op: BinOp, left: EmirValue, right: EmirValue, program: &EmirProgram) -> Expr {
    Expr::Bin {
        op,
        left: Box::new(operand(program, left)),
        right: Box::new(operand(program, right)),
    }
}
