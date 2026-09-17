//! Arithmetic, boolean, and comparison op lowering.

use super::*;
use emath_exec_ir::BuiltinId;

/// Complex binop operand: complex carriers pass through; scalars widen
/// to the tuple carrier exactly as the VM's `complex_parts` does
/// (`(x, 0.0)`); anything else refuses typed.
fn typed_operand_or_complex(
    program: &EmirProgram,
    value: EmirValue,
    kinds: &[ValueKind],
) -> Expr {
    let expr = operand(program, value);
    match kind_at(kinds, value) {
        ValueKind::Complex => expr,
        ValueKind::I64 | ValueKind::F64 => {
            let rendered = render_expr(&expr);
            Expr::Raw(format!("((({rendered}) as f64), 0.0)"))
        }
        _ => expr,
    }
}

pub(super) fn op_arith_exprs(
    op: &EmirOp,
    program: &EmirProgram,
    kinds: &[ValueKind],
) -> Result<Expr, BackendError> {
    let exact = match op {
        EmirOp::F64Add(a, b) => Some(("ratio_add", *a, *b, false)),
        EmirOp::F64Sub(a, b) => Some(("ratio_sub", *a, *b, false)),
        EmirOp::F64Mul(a, b) => Some(("ratio_mul", *a, *b, false)),
        EmirOp::F64Div(a, b) => Some(("ratio_div", *a, *b, false)),
        EmirOp::Lt(a, b) => Some(("ratio_lt", *a, *b, false)),
        EmirOp::Le(a, b) => Some(("ratio_lt", *b, *a, true)),
        EmirOp::Gt(a, b) => Some(("ratio_lt", *b, *a, false)),
        EmirOp::Ge(a, b) => Some(("ratio_lt", *a, *b, true)),
        _ => None,
    };
    if let Some((function, left, right, negate)) = exact {
        if kind_at(kinds, left) == ValueKind::Rational && kind_at(kinds, right) == ValueKind::Rational {
            let value = map_runtime_result(format!("emath_rt::{function}({}, {})", render_expr(&operand(program, left)), render_expr(&operand(program, right))));
            return Ok(if negate { Expr::Un { op: UnOp::Not, value: Box::new(value) } } else { value });
        }
        if matches!(kind_at(kinds, left), ValueKind::ExactInt | ValueKind::I64)
            && matches!(kind_at(kinds, right), ValueKind::ExactInt | ValueKind::I64)
            && (kind_at(kinds, left) == ValueKind::ExactInt
                || kind_at(kinds, right) == ValueKind::ExactInt)
        {
            let left_e = render_expr(&exact_int_operand(program, left, kinds));
            let right_e = render_expr(&exact_int_operand(program, right, kinds));
            let value = match function {
                "ratio_add" => map_runtime_result(format!("{left_e}.add(&{right_e}).map_err(|err| err.to_string())")),
                "ratio_sub" => map_runtime_result(format!("{left_e}.sub(&{right_e}).map_err(|err| err.to_string())")),
                "ratio_mul" => map_runtime_result(format!("{left_e}.mul(&{right_e}).map_err(|err| err.to_string())")),
                "ratio_div" => map_runtime_result(format!(
                    "emath_rt::exact_ratio({left_e}, {right_e}).map_err(|err| err.to_string())"
                )),
                "ratio_lt" => Expr::Raw(format!("{left_e}.cmp(&{right_e}) == core::cmp::Ordering::Less")),
                _ => {
                    return Err(BackendError::UnsupportedType(
                        "exact integer comparison is ordered".into(),
                    ));
                }
            };
            return Ok(if negate {
                Expr::Un {
                    op: UnOp::Not,
                    value: Box::new(value),
                }
            } else {
                value
            });
        }
    }
    if let EmirOp::Neg(value) = op {
        if kind_at(kinds, *value) == ValueKind::Rational {
            return Ok(map_runtime_result(format!("emath_rt::ratio_sub((0, 1), {})", render_expr(&operand(program, *value)))));
        }
    }
    // Complex-carrier arithmetic: the same EMIR ops the VM's complex arm
    // handles (interp.rs), routed to the parity-twin rt helpers.
    let complex_bin = match op {
        EmirOp::F64Add(a, b) => Some(("complex_add", *a, *b)),
        EmirOp::F64Sub(a, b) => Some(("complex_sub", *a, *b)),
        EmirOp::F64Mul(a, b) => Some(("complex_mul", *a, *b)),
        EmirOp::F64Div(a, b) => Some(("complex_div", *a, *b)),
        _ => None,
    };
    if let Some((function, left, right)) = complex_bin {
        if kind_at(kinds, left) == ValueKind::Complex
            || kind_at(kinds, right) == ValueKind::Complex
        {
            return Ok(Expr::Raw(format!(
                "emath_rt::{function}({}, {})",
                render_expr(&typed_operand_or_complex(program, left, &kinds)),
                render_expr(&typed_operand_or_complex(program, right, &kinds))
            )));
        }
    }
    if let EmirOp::Neg(value) = op {
        if kind_at(kinds, *value) == ValueKind::Complex {
            let rendered = render_expr(&operand(program, *value));
            return Ok(Expr::Raw(format!("(-({rendered}).0, -({rendered}).1)")));
        }
    }
    if let EmirOp::UnaryBuiltin(id, value) = op {
        if kind_at(kinds, *value) == ValueKind::Complex {
            let function = match id {
                BuiltinId::Sqrt => "complex_sqrt",
                BuiltinId::Ln => "complex_ln",
                BuiltinId::Exp => "complex_exp",
                other => {
                    let _ = other;
                    return Err(BackendError::UnsupportedType(
                        "unary builtin on a Complex carrier requires sqrt/ln/exp".into(),
                    ));
                }
            };
            return Ok(Expr::Raw(format!(
                "emath_rt::{function}(({arg}).0, ({arg}).1)",
                arg = render_expr(&operand(program, *value))
            )));
        }
    }
    match op {
        EmirOp::F64Add(l, r) => Ok(i64_or_f64_bin(
            BinOp::Add,
            "checked_add",
            program,
            *l,
            *r,
            &kinds,
        )),
        EmirOp::F64Sub(l, r) => Ok(i64_or_f64_bin(
            BinOp::Sub,
            "checked_sub",
            program,
            *l,
            *r,
            &kinds,
        )),
        EmirOp::F64Mul(l, r) => Ok(i64_or_f64_bin(
            BinOp::Mul,
            "checked_mul",
            program,
            *l,
            *r,
            &kinds,
        )),
        EmirOp::F64Div(l, r) => {
            if matches!(kind_at(&kinds, *l), ValueKind::I64 | ValueKind::ExactInt)
                && matches!(kind_at(&kinds, *r), ValueKind::I64 | ValueKind::ExactInt)
                && (kind_at(&kinds, *l) == ValueKind::ExactInt
                    || kind_at(&kinds, *r) == ValueKind::ExactInt)
            {
                return Ok(map_runtime_result(format!(
                    "emath_rt::exact_ratio({}, {}).map_err(|err| err.to_string())",
                    render_expr(&exact_int_operand(program, *l, &kinds)),
                    render_expr(&exact_int_operand(program, *r, &kinds))
                )));
            }
            if kind_at(&kinds, *l) == ValueKind::I64 && kind_at(&kinds, *r) == ValueKind::I64 {
                return Ok(map_runtime_result(format!(
                    "emath_rt::ratio_div((i128::from({}), 1), (i128::from({}), 1))",
                    render_expr(&operand(program, *l)),
                    render_expr(&operand(program, *r))
                )));
            }
            Ok(Expr::Bin {
                op: BinOp::Div,
                left: Box::new(typed_operand(program, *l, ValueKind::F64, &kinds)),
                right: Box::new(typed_operand(program, *r, ValueKind::F64, &kinds)),
            })
        }
        EmirOp::F64Pow(l, r) => Ok(Expr::Bin {
            op: BinOp::Pow,
            left: Box::new(typed_operand(program, *l, ValueKind::F64, &kinds)),
            right: Box::new(typed_operand(program, *r, ValueKind::F64, &kinds)),
        }),
        EmirOp::Neg(value) => {
            if operand_kind(&kinds, *value) == ValueKind::I64 {
                Ok(checked_integer_result(Expr::MethodCall {
                    receiver: Box::new(operand(program, *value)),
                    method: "checked_neg".to_string(),
                    args: Vec::new(),
                }))
            } else if operand_kind(&kinds, *value) == ValueKind::ExactInt {
                Ok(map_runtime_result(format!(
                    "{}.checked_neg().map_err(|err| err.to_string())",
                    render_expr(&operand(program, *value))
                )))
            } else {
                Ok(Expr::Un {
                    op: UnOp::Neg,
                    value: Box::new(typed_operand(program, *value, ValueKind::F64, &kinds)),
                })
            }
        }
        EmirOp::Not(value) => Ok(Expr::Un {
            op: UnOp::Not,
            value: Box::new(operand(program, *value)),
        }),
        EmirOp::UnaryBuiltin(id, value) => {
            let arg = render_expr(&typed_operand(program, *value, ValueKind::F64, &kinds));
            Ok(Expr::Raw(unary_builtin(*id, &arg)?))
        }
        EmirOp::BinaryBuiltin(id, left, right) => {
            let left = render_expr(&typed_operand(program, *left, ValueKind::F64, &kinds));
            let right = render_expr(&typed_operand(program, *right, ValueKind::F64, &kinds));
            Ok(Expr::Raw(binary_builtin(*id, &left, &right)?))
        }
        EmirOp::IsFinite(value) => Ok(Expr::MethodCall {
            receiver: Box::new(typed_operand(program, *value, ValueKind::F64, &kinds)),
            method: "is_finite".to_string(),
            args: Vec::new(),
        }),
        EmirOp::Lt(l, r) => Ok(cmp_expr(BinOp::Lt, program, *l, *r, &kinds)),
        EmirOp::Le(l, r) => Ok(cmp_expr(BinOp::Le, program, *l, *r, &kinds)),
        EmirOp::Gt(l, r) => Ok(cmp_expr(BinOp::Gt, program, *l, *r, &kinds)),
        EmirOp::Ge(l, r) => Ok(cmp_expr(BinOp::Ge, program, *l, *r, &kinds)),
        EmirOp::Eq(l, r) => Ok(cmp_expr(BinOp::Eq, program, *l, *r, &kinds)),
        EmirOp::Ne(l, r) => Ok(cmp_expr(BinOp::Ne, program, *l, *r, &kinds)),
        EmirOp::And(l, r) => Ok(comparison(BinOp::And, *l, *r, program)),
        EmirOp::Or(l, r) => Ok(comparison(BinOp::Or, *l, *r, program)),
        // `==>` = `!l || r`
        EmirOp::Imply(l, r) => Ok(Expr::Bin {
            op: BinOp::Or,
            left: Box::new(Expr::Un {
                op: UnOp::Not,
                value: Box::new(operand(program, *l)),
            }),
            right: Box::new(operand(program, *r)),
        }),
        // `<==>` = `l == r` for Bool
        EmirOp::Iff(l, r) => Ok(comparison(BinOp::Eq, *l, *r, program)),
        _ => unreachable!("op_arith_exprs routed a non-matching op"),
    }
}

fn unary_builtin(id: BuiltinId, value: &str) -> Result<String, BackendError> {
    let method = match id {
        BuiltinId::Exp => "exp",
        BuiltinId::Ln => "ln",
        BuiltinId::Sqrt => "sqrt",
        BuiltinId::Sin => "sin",
        BuiltinId::Cos => "cos",
        BuiltinId::Tan => "tan",
        BuiltinId::Tanh => "tanh",
        BuiltinId::Abs => "abs",
        BuiltinId::Floor => "floor",
        BuiltinId::Ceil => "ceil",
        BuiltinId::Round => "round",
        BuiltinId::Log2 => "log2",
        BuiltinId::Log10 => "log10",
        BuiltinId::Sinh => "sinh",
        BuiltinId::Cosh => "cosh",
        BuiltinId::Atan => "atan",
        BuiltinId::Cbrt => "cbrt",
        BuiltinId::Recip => "recip",
        BuiltinId::Fract => "fract",
        BuiltinId::Sign => {
            return Ok(format!(
                "if {value} == 0.0 {{ 0.0 }} else {{ {value}.signum() }}"
            ));
        }
        BuiltinId::Hypot | BuiltinId::Min | BuiltinId::Max | BuiltinId::Atan2 | BuiltinId::Mod => {
            return Err(BackendError::MissingArtifactContract(
                "binary builtin used as unary bytecode".to_string(),
            ));
        }
    };
    Ok(format!("{value}.{method}()"))
}

fn binary_builtin(id: BuiltinId, left: &str, right: &str) -> Result<String, BackendError> {
    match id {
        BuiltinId::Hypot => Ok(format!("{left}.hypot({right})")),
        BuiltinId::Min => Ok(format!("{left}.min({right})")),
        BuiltinId::Max => Ok(format!("{left}.max({right})")),
        BuiltinId::Atan2 => Ok(format!("{left}.atan2({right})")),
        BuiltinId::Mod => Ok(format!("{left} % {right}")),
        _ => Err(BackendError::MissingArtifactContract(
            "unary builtin used as binary bytecode".to_string(),
        )),
    }
}
