//! Runtime calls and universal register/type helpers.

use super::*;

pub(crate) fn operand(_program: &EmirProgram, value: EmirValue) -> Expr {
    Expr::Var(format!("__e{}", value.0))
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

pub(crate) fn program_may_index_fault(program: &EmirProgram) -> bool {
    program.ops.iter().any(|(op, _)| match op {
        EmirOp::VectorIndex { .. }
        | EmirOp::MatrixIndex { .. }
        | EmirOp::TensorIndex { .. }
        | EmirOp::TensorSlice { .. } => true,
        EmirOp::Fold { body, .. } => program_may_index_fault(body),
        _ => false,
    })
}

pub(super) fn index_f64(program: &EmirProgram, value: EmirValue, kinds: &[ScalarKind]) -> String {
    render_expr(&typed_operand(program, value, ScalarKind::F64, kinds))
}

pub(super) fn map_index_result(call: String) -> Expr {
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
    i64_names: &BTreeSet<String>,
) -> Result<Expr, BackendError> {
    RATE_CONTEXT.with(|cell| cell.set(true));
    let out = value_expr(program, names, states, i64_names);
    RATE_CONTEXT.with(|cell| cell.set(false));
    out
}

pub(super) fn render_slice_axis(
    program: &EmirProgram,
    axis: &EmirSliceAxis,
    kinds: &[ScalarKind],
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
    kinds: &[ScalarKind],
) -> Expr {
    let indices = indices
        .iter()
        .map(|value| index_f64(program, *value, kinds))
        .collect::<Vec<_>>()
        .join(", ");
    map_index_result(format!(
        "{{ let (__s, __d) = emath_rt::EinsumIn::einsum_operand(&{}); emath_rt::tensor_index_checked(&__s, &__d, &[{indices}]) }}",
        render_expr(&operand(program, tensor)),
    ))
}

pub(super) fn tensor_slice_call(
    program: &EmirProgram,
    tensor: EmirValue,
    axes: &[EmirSliceAxis],
    kinds: &[ScalarKind],
) -> Expr {
    let helper = slice_helper(axes);
    let axes = axes
        .iter()
        .map(|axis| render_slice_axis(program, axis, kinds))
        .collect::<Vec<_>>()
        .join(", ");
    map_index_result(format!(
        "{{ let (__s, __d) = emath_rt::EinsumIn::einsum_operand(&{}); emath_rt::{helper}(&__s, &__d, &[{axes}]) }}",
        render_expr(&operand(program, tensor)),
    ))
}

pub(super) fn register_rust_ty(
    program: &EmirProgram,
    value: EmirValue,
    kinds: &[ScalarKind],
    names: &[String],
    states: &[String],
    i64_names: &BTreeSet<String>,
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
        EmirOp::MatrixCreate { .. } => Some("Vec<Vec<f64>>".to_string()),
        EmirOp::TensorCreate { .. } => Some("emath_rt::Tensor".to_string()),
        EmirOp::SeriesCreate { .. } => Some("Vec<(f64, f64)>".to_string()),
        EmirOp::LoadInput(index) => input_rust_ty(names.get(*index as usize), i64_names),
        EmirOp::LoadState(index) => input_rust_ty(states.get(*index as usize), i64_names),
        _ => match kind_at(kinds, value) {
            ScalarKind::I64 => Some("i64".to_string()),
            ScalarKind::Bool => Some("bool".to_string()),
            ScalarKind::BigInt => Some("emath_rt::UBig".to_string()),
            ScalarKind::F64 | ScalarKind::Other => Some("f64".to_string()),
        },
    }
}

fn input_rust_ty(name: Option<&String>, i64_names: &BTreeSet<String>) -> Option<String> {
    name.map(|name| {
        if i64_names.contains(name) {
            "i64".to_string()
        } else {
            "f64".to_string()
        }
    })
}
