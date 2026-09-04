//! Scalar-kind inference and typed operand/expression helpers.

use super::*;

/// Scalar kind of an EMIR register in generated Rust. Mirrors interp:
/// I64×I64 add/sub/mul/neg and integer folds stay `i64`; everything else
/// that computes a number widens to `f64`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ScalarKind {
    I64,
    F64,
    Bool,
    /// Stage-2 (emath-t63iz): exact big field element (emath_rt::UBig).
    BigInt,
    Other,
}

pub(crate) fn program_kind(
    program: &EmirProgram,
    names: &[String],
    states: &[String],
    i64_names: &BTreeSet<String>,
) -> ScalarKind {
    if program.ops.is_empty() {
        return ScalarKind::Other;
    }
    let kinds = scalar_kinds(program, names, states, i64_names);
    kinds
        .get(program.result.0 as usize)
        .copied()
        .unwrap_or(ScalarKind::Other)
}

pub(super) fn scalar_kinds(
    program: &EmirProgram,
    names: &[String],
    states: &[String],
    i64_names: &BTreeSet<String>,
) -> Vec<ScalarKind> {
    let n = program.ops.len();
    let mut kinds = vec![ScalarKind::Other; n];
    for (i, (op, _)) in program.ops.iter().enumerate() {
        kinds[i] = kind_of_op(op, &kinds, names, states, i64_names);
    }
    kinds
}

pub(super) fn kind_at(kinds: &[ScalarKind], value: EmirValue) -> ScalarKind {
    kinds
        .get(value.0 as usize)
        .copied()
        .unwrap_or(ScalarKind::Other)
}

pub(super) fn kind_of_op(
    op: &EmirOp,
    kinds: &[ScalarKind],
    names: &[String],
    states: &[String],
    i64_names: &BTreeSet<String>,
) -> ScalarKind {
    match op {
        EmirOp::ConstI64(_) => ScalarKind::I64,
        EmirOp::ConstBigInt(_) => ScalarKind::BigInt,
        EmirOp::ConstF64(_) => ScalarKind::F64,
        EmirOp::ConstBool(_) => ScalarKind::Bool,
        EmirOp::ConstText(_)
        | EmirOp::ConstComplex(..)
        | EmirOp::FormatText { .. }
        | EmirOp::SeriesCreate { .. }
        | EmirOp::SetCreate { .. }
        | EmirOp::RecordCreate { .. }
        | EmirOp::VectorCreate(_)
        | EmirOp::MatrixCreate { .. }
        | EmirOp::TensorCreate { .. }
        | EmirOp::TensorSlice { .. }
        | EmirOp::OptionSome(_)
        | EmirOp::OptionNone
        | EmirOp::ResultOk(_)
        | EmirOp::ResultErr(_)
        | EmirOp::ResultErrorOf(_)
        | EmirOp::ProgramLiteral(_)
        | EmirOp::ApplyCapability { .. }
        | EmirOp::VectorMap { .. }
        | EmirOp::VectorMapScalar { .. }
        | EmirOp::VectorReduce { .. } => ScalarKind::Other,
        EmirOp::SeriesSample { .. }
        | EmirOp::F64Div(..)
        | EmirOp::F64Pow(..)
        | EmirOp::UnaryBuiltin(..)
        | EmirOp::BinaryBuiltin(..)
        | EmirOp::VectorIndex { .. }
        | EmirOp::MatrixIndex { .. }
        | EmirOp::TensorIndex { .. } => ScalarKind::F64,
        EmirOp::LoadInput(index) => input_kind(names.get(*index as usize), i64_names),
        EmirOp::LoadState(index) => input_kind(states.get(*index as usize), i64_names),
        EmirOp::F64Add(left, right) | EmirOp::F64Sub(left, right) | EmirOp::F64Mul(left, right) => {
            if kind_at(kinds, *left) == ScalarKind::I64 && kind_at(kinds, *right) == ScalarKind::I64
            {
                ScalarKind::I64
            } else {
                ScalarKind::F64
            }
        }
        EmirOp::Neg(value) => kind_at(kinds, *value),
        EmirOp::IsFinite(_)
        | EmirOp::Eq(..)
        | EmirOp::Ne(..)
        | EmirOp::Lt(..)
        | EmirOp::Le(..)
        | EmirOp::Gt(..)
        | EmirOp::Ge(..)
        | EmirOp::And(..)
        | EmirOp::Or(..)
        | EmirOp::Imply(..)
        | EmirOp::Iff(..)
        | EmirOp::SetContains { .. }
        | EmirOp::Not(_)
        | EmirOp::OptionIsSome(_)
        | EmirOp::ResultIsOk(_)
        | EmirOp::VectorAllFinite(_) => ScalarKind::Bool,
        EmirOp::Select {
            then_value,
            else_value,
            ..
        } => {
            let then_kind = kind_at(kinds, *then_value);
            let else_kind = kind_at(kinds, *else_value);
            if then_kind == else_kind {
                then_kind
            } else {
                ScalarKind::F64
            }
        }
        EmirOp::Fold {
            combine,
            init,
            loop_var_index,
            body,
            ..
        } => match combine {
            FoldCombine::And | FoldCombine::Or => ScalarKind::Bool,
            FoldCombine::Add | FoldCombine::Mul => {
                if fold_is_i64(
                    kinds,
                    *init,
                    *loop_var_index,
                    body,
                    names,
                    states,
                    i64_names,
                ) {
                    ScalarKind::I64
                } else {
                    ScalarKind::F64
                }
            }
        },
        EmirOp::OptionUnwrapOr(_, default) | EmirOp::ResultUnwrapOr(_, default) => {
            kind_at(kinds, *default)
        }
    }
}

fn input_kind(name: Option<&String>, i64_names: &BTreeSet<String>) -> ScalarKind {
    if name.is_some_and(|name| i64_names.contains(name)) {
        ScalarKind::I64
    } else {
        ScalarKind::F64
    }
}

pub(super) fn fold_is_i64(
    kinds: &[ScalarKind],
    init: EmirValue,
    loop_var_index: u16,
    body: &EmirProgram,
    names: &[String],
    states: &[String],
    i64_names: &BTreeSet<String>,
) -> bool {
    if kind_at(kinds, init) != ScalarKind::I64 {
        return false;
    }
    let mut body_names = names.to_vec();
    let lv = loop_var_index as usize;
    while body_names.len() <= lv {
        body_names.push(String::new());
    }
    // Name the loop variable by its slot index: nested binders each own a
    // distinct index, so per-index names cannot shadow an outer binder's
    // variable (a shared name made inner bodies read the inner loop var).
    let loop_name = format!("__loop{lv}");
    body_names[lv] = loop_name.clone();
    let mut body_i64 = i64_names.clone();
    body_i64.insert(loop_name);
    program_kind(body, &body_names, states, &body_i64) == ScalarKind::I64
}

pub(super) fn as_f64(expr: Expr) -> Expr {
    Expr::Raw(format!("({}) as f64", render_expr(&expr)))
}

pub(super) fn as_i64(expr: Expr) -> Expr {
    Expr::Raw(format!("({}) as i64", render_expr(&expr)))
}

pub(super) fn operand_kind<'a>(kinds: &'a [ScalarKind], value: EmirValue) -> ScalarKind {
    kind_at(kinds, value)
}

pub(super) fn typed_operand(
    program: &EmirProgram,
    value: EmirValue,
    want: ScalarKind,
    kinds: &[ScalarKind],
) -> Expr {
    let expr = operand(program, value);
    match (operand_kind(kinds, value), want) {
        (ScalarKind::I64, ScalarKind::F64) => as_f64(expr),
        (ScalarKind::F64, ScalarKind::I64) => as_i64(expr),
        _ => expr,
    }
}

pub(super) fn i64_checked_bin(method: &str, left: Expr, right: Expr) -> Expr {
    Expr::MethodCall {
        receiver: Box::new(Expr::MethodCall {
            receiver: Box::new(left),
            method: method.to_string(),
            args: vec![right],
        }),
        method: "expect".to_string(),
        args: vec![Expr::Str("i64 overflow".to_string())],
    }
}

pub(super) fn cmp_expr(
    op: BinOp,
    program: &EmirProgram,
    left: EmirValue,
    right: EmirValue,
    kinds: &[ScalarKind],
) -> Expr {
    let lk = operand_kind(kinds, left);
    let rk = operand_kind(kinds, right);
    match (lk, rk) {
        (ScalarKind::I64, ScalarKind::I64) | (ScalarKind::Bool, ScalarKind::Bool) => Expr::Bin {
            op,
            left: Box::new(operand(program, left)),
            right: Box::new(operand(program, right)),
        },
        (ScalarKind::I64, ScalarKind::F64) => mixed_i64_f64_cmp(
            op,
            operand(program, left),
            typed_operand(program, right, ScalarKind::F64, kinds),
            true,
        ),
        (ScalarKind::F64, ScalarKind::I64) => mixed_i64_f64_cmp(
            op,
            operand(program, right),
            typed_operand(program, left, ScalarKind::F64, kinds),
            false,
        ),
        (ScalarKind::I64, _) => Expr::Bin {
            op,
            left: Box::new(as_f64(operand(program, left))),
            right: Box::new(typed_operand(program, right, ScalarKind::F64, kinds)),
        },
        (_, ScalarKind::I64) => Expr::Bin {
            op,
            left: Box::new(typed_operand(program, left, ScalarKind::F64, kinds)),
            right: Box::new(as_f64(operand(program, right))),
        },
        _ => Expr::Bin {
            op,
            left: Box::new(operand(program, left)),
            right: Box::new(operand(program, right)),
        },
    }
}

/// Mixed Int/Float64 compare must not widen through `as f64` (2^53 lie).
pub(super) fn mixed_i64_f64_cmp(
    op: BinOp,
    int_expr: Expr,
    float_expr: Expr,
    int_on_left: bool,
) -> Expr {
    let eq = rt_call("eq_i64_f64", vec![int_expr.clone(), float_expr.clone()]);
    let cmp = render_expr(&rt_call("cmp_i64_f64", vec![int_expr, float_expr]));
    let (lt, gt) = if int_on_left {
        ("Less", "Greater")
    } else {
        ("Greater", "Less")
    };
    match op {
        BinOp::Eq => eq,
        BinOp::Ne => Expr::Un {
            op: UnOp::Not,
            value: Box::new(eq),
        },
        BinOp::Lt => Expr::Raw(format!("{cmp} == Some(core::cmp::Ordering::{lt})")),
        BinOp::Gt => Expr::Raw(format!("{cmp} == Some(core::cmp::Ordering::{gt})")),
        BinOp::Le => Expr::Raw(format!(
            "matches!({cmp}, Some(core::cmp::Ordering::{lt} | core::cmp::Ordering::Equal))"
        )),
        BinOp::Ge => Expr::Raw(format!(
            "matches!({cmp}, Some(core::cmp::Ordering::{gt} | core::cmp::Ordering::Equal))"
        )),
        BinOp::Add
        | BinOp::Sub
        | BinOp::Mul
        | BinOp::Div
        | BinOp::Rem
        | BinOp::Pow
        | BinOp::And
        | BinOp::Or => unreachable!("cmp_expr only emits comparison BinOps"),
    }
}

pub(super) fn i64_or_f64_bin(
    f64_op: BinOp,
    i64_method: &str,
    program: &EmirProgram,
    left: EmirValue,
    right: EmirValue,
    kinds: &[ScalarKind],
) -> Expr {
    if operand_kind(kinds, left) == ScalarKind::I64 && operand_kind(kinds, right) == ScalarKind::I64
    {
        i64_checked_bin(i64_method, operand(program, left), operand(program, right))
    } else {
        Expr::Bin {
            op: f64_op,
            left: Box::new(typed_operand(program, left, ScalarKind::F64, kinds)),
            right: Box::new(typed_operand(program, right, ScalarKind::F64, kinds)),
        }
    }
}

pub(crate) fn coerce_to_ty(expr: Expr, from: ScalarKind, to: &Ty) -> Expr {
    match (from, to) {
        (ScalarKind::I64, Ty::F64) => as_f64(expr),
        (ScalarKind::F64, Ty::I64) => as_i64(expr),
        _ => expr,
    }
}

/// Render the program as an expression. Multi-op programs become a block
/// `{ let __e0 = ...; ...; __eN }`; single-op programs inline directly.
pub(crate) fn value_expr(
    program: &EmirProgram,
    names: &[String],
    states: &[String],
    i64_names: &BTreeSet<String>,
) -> Result<Expr, BackendError> {
    if program.ops.len() == 1 {
        return op_expr(&program.ops[0].0, program, names, states, i64_names);
    }
    let flat = flat_ssa(program, names, states, i64_names, None)?;
    let mut statements: Vec<Stmt> = Vec::with_capacity(flat.e_lets.len() + 1);
    for (pattern, src) in flat.e_lets {
        statements.push(Stmt::Let {
            pattern,
            value: Box::new(Expr::Raw(src)),
        });
    }
    statements.push(Stmt::Expr(Expr::Raw(flat.e_tail)));
    Ok(Expr::Block(Box::new(Stmt::Block(Block { statements }))))
}
