//! Runtime calls and universal register/type helpers.

use super::*;

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
    } else if let Ok(ty) = kind.rust_ty() {
        Expr::Raw(format!("{{ let __owned_source = &{}; <{} as Clone>::clone(__owned_source) }}", render_expr(&expression), crate::rust_ir::render::render_ty(&ty)))
    } else { clone_expr(expression) }
}

/// Keep one reference layer at load boundaries, including borrowed captures.
pub(super) fn borrowed_value(value: Expr, kind: &ValueKind) -> Expr {
    let expression = render_expr(&value);
    match kind.borrowed_rust_ty() {
        Ok(ty) => Expr::Raw(format!("{{ let __borrow: &{} = &{expression}; __borrow }}", crate::rust_ir::render::render_ty(&ty))),
        Err(_) => Expr::Raw(format!("&{expression}")),
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
pub(super) struct ReferenceScope(bool);
impl ReferenceScope {
    pub(super) fn enter() -> Self { Self(REFERENCE_CONTEXT.with(|context| context.replace(true))) }
}
impl Drop for ReferenceScope {
    fn drop(&mut self) { REFERENCE_CONTEXT.with(|context| context.set(self.0)); }
}
pub(super) fn checked_integer_result(call: Expr) -> Expr {
    if REFERENCE_CONTEXT.with(std::cell::Cell::get) {
        map_runtime_result(format!("{}.ok_or(\"i64 overflow\")", render_expr(&call)))
    } else {
        Expr::MethodCall { receiver: Box::new(call), method: "expect".into(), args: vec![Expr::Str("i64 overflow".into())] }
    }
}

pub(crate) fn program_may_fault(program: &EmirProgram) -> bool {
    program.ops.iter().any(|(op, _)| match op {
        EmirOp::Branch { then_body, else_body, .. } => program_may_fault(then_body) || program_may_fault(else_body),
        EmirOp::CallFrame { .. } | EmirOp::DenseRepack { .. } | EmirOp::DenseValues(_)
        | EmirOp::VectorSlice { .. } | EmirOp::VectorConcat(_)
        | EmirOp::Iterate { .. } | EmirOp::Collect { .. } | EmirOp::Refuse(_) | EmirOp::RefuseValue(_) | EmirOp::ToInt(_) | EmirOp::IntegerQuotient(_, _) | EmirOp::CallProgram { .. } | EmirOp::CallScalarProgram { .. } | EmirOp::CallRealProgram { .. } | EmirOp::TryCallRealProgram { .. } => true,
        EmirOp::VectorIndex { .. }
        | EmirOp::MatrixCreate { .. }
        | EmirOp::MatrixRows(_)
        | EmirOp::MatrixCols(_)
        | EmirOp::MatrixPack { .. }
        | EmirOp::TensorShape(_)
        | EmirOp::F64PowI(..)
        | EmirOp::TextTrim(_)
        | EmirOp::TextLength(_)
        | EmirOp::TextByte(..)
        | EmirOp::FormatScientific(..)
        | EmirOp::ParseF64(_)
        | EmirOp::TensorPack { .. }
        | EmirOp::DenseIndex { .. }
        | EmirOp::MatrixIndex { .. }
        | EmirOp::TensorIndex { .. }
        | EmirOp::TensorSlice { .. } => true,
        EmirOp::Fold { body, .. } => program_may_fault(body),
        EmirOp::ApplyCapability { capability, .. } => {
            if emath_exec_ir::native_kernel::checked::verified(capability).is_some() { return true; }
            if super::artifact_may_fault(capability) { return true; }
            emath_exec_ir::native_kernel::installed_reference_cell(capability).is_some_and(|cell| {
                emath_exec_ir::native_kernel::installed_signature(capability).is_some_and(|signature| signature.inputs.iter().chain(std::iter::once(&signature.output)).any(|ty| matches!(ty.as_str(), "Int" | "Nat" | "I64")))
                    || cell.program.ops.iter().any(|(op, _)| matches!(op, EmirOp::ConstI64(_)))
                    || cell.params.iter().any(|(_, shape)| *shape == emath_exec_ir::term_compile::ParamShape::Rational)
                    || cell.defaults.iter().any(|default| program_may_fault(default) || default.ops.iter().any(|(op, _)| matches!(op, EmirOp::ConstI64(_))))
                    || program_may_fault(&cell.program)
            })
        }
        _ => false,
    })
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
    if rate_context() {
        return Expr::Raw(format!(
            "{call}.map_err(|e| e.to_string()).expect(\"internal: checked-op fault on admitted model\")"
        ));
    }
    Expr::Raw(format!("{call}.map_err(|e| e.to_string())?"))
}

thread_local! {
    static RATE_CONTEXT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static FOLD_CONTEXT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub(super) fn rate_context() -> bool {
    RATE_CONTEXT.with(std::cell::Cell::get)
}

pub(super) fn fold_context() -> bool {
    FOLD_CONTEXT.with(std::cell::Cell::get)
}

pub(super) fn set_fold_context(value: bool) {
    FOLD_CONTEXT.with(|cell| cell.set(value));
}

pub(crate) fn value_expr_rate(
    program: &EmirProgram,
    names: &[String],
    states: &[String],
    input_kinds: &InputKinds,
) -> Result<Expr, BackendError> {
    RATE_CONTEXT.with(|cell| cell.set(true));
    let out = value_expr(program, names, states, input_kinds);
    RATE_CONTEXT.with(|cell| cell.set(false));
    out
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

pub(super) fn tensor_index_call(
    program: &EmirProgram,
    tensor: EmirValue,
    indices: &[EmirValue],
    kinds: &[ValueKind],
) -> Expr {
    let indices = indices
        .iter()
        .map(|value| index_f64(program, *value, kinds))
        .collect::<Vec<_>>()
        .join(", ");
    map_runtime_result(format!(
        "{{ let (__s, __d) = emath_rt::EinsumIn::einsum_operand(&{}); emath_rt::tensor_index_checked(&__s, &__d, &[{indices}]) }}",
        render_expr(&operand(program, tensor)),
    ))
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

pub(super) fn register_rust_ty(
    program: &EmirProgram,
    value: EmirValue,
    kinds: &[ValueKind],
    names: &[String],
    states: &[String],
    input_kinds: &InputKinds,
) -> Option<String> {
    let op = &program.ops.get(value.0 as usize)?.0;
    match op {
        EmirOp::ConstI64(_) => Some("i64".to_string()),
        EmirOp::ConstF64(_) | EmirOp::SeriesSample { .. } => Some("f64".to_string()),
        EmirOp::ConstBool(_)
        | EmirOp::SetContains { .. }
        | EmirOp::OptionIsSome(_)
        | EmirOp::ResultIsOk(_)
        | EmirOp::VectorAllFinite(_)
        | EmirOp::Fold {
            combine: FoldCombine::And | FoldCombine::Or,
            ..
        } => Some("bool".to_string()),
        EmirOp::ConstText(_) | EmirOp::FormatText { .. } => Some("String".to_string()),
        EmirOp::ConstComplex(..) => Some("(f64, f64)".to_string()),
        EmirOp::VectorCreate(_) | EmirOp::VectorMap { .. } | EmirOp::VectorMapScalar { .. } => {
            Some("Vec<f64>".to_string())
        }
        EmirOp::MatrixCreate { .. } | EmirOp::MatrixPack { .. } => Some("emath_rt::Matrix".to_string()),
        EmirOp::TensorCreate { .. } => Some("emath_rt::Tensor".to_string()),
        EmirOp::SeriesCreate { .. } => Some("Vec<(f64, f64)>".to_string()),
        EmirOp::LoadInput(index) => input_rust_ty(names.get(*index as usize), input_kinds),
        EmirOp::LoadState(index) => input_rust_ty(states.get(*index as usize), input_kinds),
        _ => kind_at(kinds, value).rust_ty().ok().map(|ty| crate::rust_ir::render::render_ty(&ty)),
    }
}

fn input_rust_ty(name: Option<&String>, input_kinds: &InputKinds) -> Option<String> {
    name.and_then(|name| input_kinds.get(name)).and_then(|kind| kind.rust_ty().ok()).map(|ty| crate::rust_ir::render::render_ty(&ty))
}

thread_local! { static LOCAL_STATE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
pub(super) struct LocalStateScope(bool);
impl LocalStateScope {
    pub(super) fn enter() -> Self { Self(LOCAL_STATE.with(|state| state.replace(true))) }
}
impl Drop for LocalStateScope {
    fn drop(&mut self) { LOCAL_STATE.with(|state| state.set(self.0)); }
}
pub(super) fn local_state() -> bool { LOCAL_STATE.with(std::cell::Cell::get) }
