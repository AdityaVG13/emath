//! Runtime calls and universal register/type helpers.

use super::{EmirProgram, EmirValue, Expr, ValueKind, kind_at, render_expr, EmirOp, typed_operand, BackendError, EmirSliceAxis, INT_LANE_FAULT};

pub(crate) fn operand(_program: &EmirProgram, value: EmirValue) -> Expr {
    Expr::Var(format!("__e{}", value.0))
}

pub(super) fn clone_expr(value: Expr) -> Expr {
    Expr::MethodCall { receiver: Box::new(value), method: "clone".into(), args: Vec::new() }
}

pub(super) fn owned_operand(program: &EmirProgram, value: EmirValue, kinds: &[ValueKind]) -> Expr {
    owned_value(operand(program, value), &kind_at(kinds, value))
}

/// Materialize borrowed carriers only at an owning value boundary.
pub(super) fn owned_value(expression: Expr, kind: &ValueKind) -> Expr {
    if kind.is_copy() { expression } else if *kind == ValueKind::Text {
        Expr::MethodCall { receiver: Box::new(expression), method: "to_string".into(), args: Vec::new() }
    } else if matches!(kind, ValueKind::Closure { .. }) {
        // Closures are shared `Rc` handles: an owning boundary clones
        // the handle (never a deep clone, never a borrow).
        Expr::Raw(format!("std::rc::Rc::clone(&{})", render_expr(&expression)))
    } else if let Ok(ty) = kind.rust_ty() {
        Expr::Raw(format!("{{ let __owned_source = &{}; <{} as Clone>::clone(__owned_source) }}", render_expr(&expression), crate::rust_ir::render::render_ty(&ty)))
    } else { clone_expr(expression) }
}

/// Keep one reference layer at load boundaries, including borrowed captures.
pub(super) fn borrowed_value(value: Expr, kind: &ValueKind) -> Expr {
    let expression = render_expr(&value);
    if matches!(kind, ValueKind::Closure { .. }) {
        // A closure carrier is a shared `Rc<dyn Fn>` handle: loading
        // it clones the handle so multi-use registers never move the
        // scope binding.
        return Expr::Raw(format!("std::rc::Rc::clone(&{expression})"));
    }
    match kind.borrowed_rust_ty() {
        Ok(ty) => Expr::Raw(format!("{{ let __borrow: &{} = &{expression}; __borrow }}", crate::rust_ir::render::render_ty(&ty))),
        Err(_) => Expr::Raw(format!("&{expression}")),
    }
}

/// True when a register's defining op renders as a borrowed
/// expression: the non-copy load lane (LoadInput/LoadState) renders
/// `{ let __borrow: &T = ..; __borrow }`, a generic record
/// projection (RecordField's fallback arm) borrows its non-copy
/// field result, and a checked sequence index of a non-copy element
/// (or a matrix row) keeps the `Option::get` reference - all bound
/// to a register token and inlined at a single use. (The Node,
/// Rational-part, and sequence-`length` projection arms render owned
/// values; index lanes that `.cloned()`/`.copied()` render owned
/// values.) Consumers filling a `&T` method-argument slot must not
/// add another reference layer on top.
pub(super) fn borrowed_register(
    program: &EmirProgram,
    value: EmirValue,
    kinds: &[ValueKind],
) -> bool {
    if kind_at(kinds, value).is_copy() {
        return false;
    }
    match program.ops.get(value.0 as usize).map(|(op, _)| op) {
        Some(EmirOp::LoadInput(_)) => true,
        Some(EmirOp::RecordField { record, field }) => {
            let receiver = kind_at(kinds, *record);
            !(receiver == ValueKind::Node
                || receiver == ValueKind::Rational
                || (field == "length"
                    && matches!(
                        receiver,
                        ValueKind::Vector(_) | ValueKind::Matrix(_) | ValueKind::Tensor
                    )))
        }
        Some(EmirOp::VectorIndex { vector, .. }) => {
            // Mirror the render's materialize lane exactly: an
            // ExactInt element and copy elements clone out of the
            // `Option` (owned results); non-copy elements and matrix
            // rows keep the `Option::get` reference.
            match kind_at(kinds, *vector) {
                ValueKind::Vector(element) => {
                    *element != ValueKind::ExactInt && !element.is_copy()
                }
                ValueKind::Matrix(_) => true,
                _ => false,
            }
        }
        _ => false,
    }
}

pub(super) fn operand_ref(program: &EmirProgram, value: EmirValue) -> Expr {
    Expr::Raw(format!("&{}", render_expr(&operand(program, value))))
}

pub(super) fn rt_call(name: &str, args: Vec<Expr>) -> Expr {
    Expr::Call {
        path: vec!["emath_rt".to_string(), name.to_string()],
        args,
    }
}

thread_local! { static REFERENCE_CONTEXT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
pub(crate) struct ReferenceScope(bool);
impl ReferenceScope {
    pub(crate) fn enter() -> Self { Self(REFERENCE_CONTEXT.with(|context| context.replace(true))) }
}
impl Drop for ReferenceScope {
    fn drop(&mut self) { REFERENCE_CONTEXT.with(|context| context.set(self.0)); }
}
thread_local! {
    static SHARED_CAPTURES: std::cell::RefCell<Vec<(usize, Vec<Option<String>>)>> =
        const { std::cell::RefCell::new(Vec::new()) };
}
/// While a typed program literal's body renders, names the `Rc` that
/// holds each of its shared captures (indexed by body input), so a
/// literal nested in that body re-shares the capture instead of
/// copying it every time the nested literal is rebuilt.
pub(crate) struct SharedCaptureScope;
impl SharedCaptureScope {
    pub(crate) fn depth() -> usize {
        SHARED_CAPTURES.with(|stack| stack.borrow().len())
    }
    pub(crate) fn enter(body: &EmirProgram, shared: Vec<Option<String>>) -> Self {
        SHARED_CAPTURES.with(|stack| stack.borrow_mut().push((std::ptr::from_ref(body).addr(), shared)));
        Self
    }
    /// The enclosing `Rc` behind `value` when it loads a shared capture
    /// of the literal body being rendered.
    pub(crate) fn source(program: &EmirProgram, value: EmirValue) -> Option<String> {
        let Some((EmirOp::LoadInput(index), _)) = program.ops.get(value.0 as usize) else {
            return None;
        };
        let key = std::ptr::from_ref(program).addr();
        SHARED_CAPTURES.with(|stack| {
            let stack = stack.borrow();
            let (_, shared) = stack.iter().rev().find(|(body, _)| *body == key)?;
            shared.get(usize::from(*index)).cloned().flatten()
        })
    }
}
impl Drop for SharedCaptureScope {
    fn drop(&mut self) { SHARED_CAPTURES.with(|stack| { stack.borrow_mut().pop(); }); }
}
pub(super) fn checked_integer_result(call: Expr) -> Expr {
    map_runtime_result(format!("{}.ok_or({INT_LANE_FAULT:?})", render_expr(&call)))
}

pub(super) fn index_f64(program: &EmirProgram, value: EmirValue, kinds: &[ValueKind]) -> String {
    render_expr(&typed_operand(program, value, ValueKind::F64, kinds))
}

pub(super) fn map_runtime_result(call: String) -> Expr {
    if REFERENCE_CONTEXT.with(std::cell::Cell::get) {
        return Expr::Raw(format!("{call}.map_err(|e| e.to_string())?"));
    }
    if fold_context() {
        return Expr::Raw(format!(
            "{call}.map_err(|e| e.to_string()).unwrap_or_else(|e| panic!(\"{{e}}\"))"
        ));
    }
    Expr::Raw(format!("{call}.map_err(|e| e.to_string())?"))
}

thread_local! {
    static FOLD_CONTEXT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub(super) fn fold_context() -> bool {
    FOLD_CONTEXT.with(std::cell::Cell::get)
}

pub(super) fn set_fold_context(value: bool) {
    FOLD_CONTEXT.with(|cell| cell.set(value));
}

pub(super) fn render_slice_axis(
    program: &EmirProgram,
    axis: &EmirSliceAxis,
    kinds: &[ValueKind],
) -> String {
    match *axis {
        EmirSliceAxis::Point(value) => format!(
            "emath_rt::SliceAxis::Point({})",
            index_f64(program, value, kinds)
        ),
        EmirSliceAxis::Range { start, end } => format!(
            "emath_rt::SliceAxis::Range {{ start: {}, end: {} }}",
            index_f64(program, start, kinds),
            index_f64(program, end, kinds)
        ),
    }
}

fn slice_helper(axes: &[EmirSliceAxis]) -> &'static str {
    match axes
        .iter()
        .filter(|axis| matches!(axis, EmirSliceAxis::Range { .. }))
        .count()
    {
        0 => "tensor_slice_as_scalar",
        1 => "tensor_slice_as_vector",
        2 => "tensor_slice_as_matrix",
        _ => "tensor_slice_as_tensor",
    }
}

pub(super) fn tensor_slice_call(
    program: &EmirProgram,
    tensor: EmirValue,
    axes: &[EmirSliceAxis],
    kinds: &[ValueKind],
) -> Expr {
    let helper = slice_helper(axes);
    let axes = axes
        .iter()
        .map(|axis| render_slice_axis(program, axis, kinds))
        .collect::<Vec<_>>()
        .join(", ");
    map_runtime_result(format!(
        "{{ let (__s, __d) = emath_rt::EinsumIn::einsum_operand(&{}); emath_rt::{helper}(&__s, &__d, &[{axes}]) }}",
        render_expr(&operand(program, tensor)),
    ))
}

pub(super) fn exact_int_operand(
    program: &EmirProgram,
    value: EmirValue,
    kinds: &[ValueKind],
) -> Expr {
    match kind_at(kinds, value) {
        ValueKind::ExactInt => operand(program, value),
        ValueKind::I64 => Expr::Raw(format!(
            "emath_rt::ExactInt::from({})",
            render_expr(&operand(program, value))
        )),
        ValueKind::Rational => {
            // A ratio-typed register at an exact-integer machine op
            // (an Int datum routed through a Rat-declared record
            // field projects the layout's ratio carrier): project
            // value-exactly - a denominator-1 ratio is the integer it
            // equals, anything else refuses by name (the VM's Int
            // parameter admits integer values only). The `?` form
            // propagates in both fn and closure contexts, unlike a
            // bare `return`.
            let ratio = render_expr(&operand(program, value));
            Expr::Raw(format!(
                "{{ let __ratio = {ratio}; __ratio.1.is_one().then(|| __ratio.0.clone()).ok_or(\"E-INT-003: exact integer operand is a non-integral rational\")? }}"
            ))
        }
        kind @ (ValueKind::CodeValue | ValueKind::Evaluated | ValueKind::Node | ValueKind::Cell) => {
            // A union-valued operand (an evaluated quoted expression)
            // at an exact-integer machine-op slot: the checked
            // projection - Int admits, anything else refuses by name.
            Expr::Raw(format!(
                "emath_rt::code::project_exact_int(&{}, \"exact integer operand\")?",
                render_expr(&super::to_code_value(operand(program, value), &kind)
                    .expect("dynamic scalar carrier conversion"))
            ))
        }
        _ => operand(program, value),
    }
}

/// An operand rendered for a `&ExactInt` slot. A borrowed ExactInt
/// register (the non-copy load lane, a borrowed record projection)
/// already renders as exactly one reference layer, so the `&` is added
/// only for owned expressions; every other kind converts to an owned
/// ExactInt first and always takes the `&`.
pub(super) fn exact_int_ref(program: &EmirProgram, value: EmirValue, kinds: &[ValueKind]) -> String {
    let rendered = render_expr(&exact_int_operand(program, value, kinds));
    if kind_at(kinds, value) == ValueKind::ExactInt && borrowed_register(program, value, kinds) {
        rendered
    } else {
        format!("&{rendered}")
    }
}

pub(super) fn exact_int_call_expr(
    name: &str,
    args: &[EmirValue],
    program: &EmirProgram,
    kinds: &[ValueKind],
) -> Result<Expr, BackendError> {
    let as_int = |index: usize| -> Result<String, BackendError> {
        args.get(index)
            .map(|arg| render_expr(&exact_int_operand(program, *arg, kinds)))
            .ok_or_else(|| BackendError::UnsupportedType(format!("{name} argument count")))
    };
    let as_int_ref = |index: usize| -> Result<String, BackendError> {
        args.get(index)
            .map(|arg| exact_int_ref(program, *arg, kinds))
            .ok_or_else(|| BackendError::UnsupportedType(format!("{name} argument count")))
    };
    let as_seq = |index: usize| -> Result<String, BackendError> {
        let arg = args
            .get(index)
            .ok_or_else(|| BackendError::UnsupportedType(format!("{name} argument count")))?;
        Ok(match kind_at(kinds, *arg) {
            ValueKind::Vector(inner) if *inner == ValueKind::ExactInt => {
                format!("({}).clone()", render_expr(&operand(program, *arg)))
            }
            // Union elements project to Int by name, as the VM's Int
            // sum refuses a non-integer element.
            ValueKind::Vector(inner) if *inner == ValueKind::CodeValue => format!(
                "({}).iter().map(|item| emath_rt::code::project_exact_int(item, \"sequence element\")).collect::<Result<Vec<_>, String>>()?",
                render_expr(&operand(program, *arg))
            ),
            _ => format!(
                "({}).iter().cloned().map(emath_rt::ExactInt::from).collect::<Vec<_>>()",
                render_expr(&operand(program, *arg))
            ),
        })
    };
    let code = match name {
        "int_quot" => format!(
            "{}.quot({}).map_err(|err| err.to_string())",
            as_int(0)?,
            as_int_ref(1)?
        ),
        "int_rem" => format!(
            "{}.rem_euclid({}).map_err(|err| err.to_string())",
            as_int(0)?,
            as_int_ref(1)?
        ),
        "int_pow" => format!(
            "{}.pow({}).map_err(|err| err.to_string())",
            as_int(0)?,
            as_int_ref(1)?
        ),
        "int_sum" => format!(
            "emath_rt::exact_int_sum(&{}).map_err(|err| err.to_string())",
            as_seq(0)?
        ),
        "int_prod" => format!(
            "emath_rt::exact_int_prod(&{}).map_err(|err| err.to_string())",
            as_seq(0)?
        ),
        other => {
            return Err(BackendError::UnsupportedType(format!(
                "unknown exact integer op `{other}`"
            )));
        }
    };
    Ok(map_runtime_result(code))
}

