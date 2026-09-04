//! Arithmetic, boolean, and comparison op lowering.

use super::*;
use emath_exec_ir::BuiltinId;

pub(super) fn op_arith_exprs(
    op: &EmirOp,
    program: &EmirProgram,
    kinds: &[ScalarKind],
) -> Result<Expr, BackendError> {
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
        EmirOp::F64Div(l, r) => Ok(Expr::Bin {
            op: BinOp::Div,
            left: Box::new(typed_operand(program, *l, ScalarKind::F64, &kinds)),
            right: Box::new(typed_operand(program, *r, ScalarKind::F64, &kinds)),
        }),
        EmirOp::F64Pow(l, r) => Ok(Expr::Bin {
            op: BinOp::Pow,
            left: Box::new(typed_operand(program, *l, ScalarKind::F64, &kinds)),
            right: Box::new(typed_operand(program, *r, ScalarKind::F64, &kinds)),
        }),
        EmirOp::Neg(value) => {
            if operand_kind(&kinds, *value) == ScalarKind::I64 {
                Ok(Expr::MethodCall {
                    receiver: Box::new(Expr::MethodCall {
                        receiver: Box::new(operand(program, *value)),
                        method: "checked_neg".to_string(),
                        args: Vec::new(),
                    }),
                    method: "expect".to_string(),
                    args: vec![Expr::Str("i64 overflow".to_string())],
                })
            } else {
                Ok(Expr::Un {
                    op: UnOp::Neg,
                    value: Box::new(typed_operand(program, *value, ScalarKind::F64, &kinds)),
                })
            }
        }
        EmirOp::Not(value) => Ok(Expr::Un {
            op: UnOp::Not,
            value: Box::new(operand(program, *value)),
        }),
        EmirOp::UnaryBuiltin(id, value) => {
            let arg = render_expr(&typed_operand(program, *value, ScalarKind::F64, &kinds));
            Ok(Expr::Raw(unary_builtin(*id, &arg)?))
        }
        EmirOp::BinaryBuiltin(id, left, right) => {
            let left = render_expr(&typed_operand(program, *left, ScalarKind::F64, &kinds));
            let right = render_expr(&typed_operand(program, *right, ScalarKind::F64, &kinds));
            Ok(Expr::Raw(binary_builtin(*id, &left, &right)?))
        }
        EmirOp::IsFinite(value) => Ok(Expr::MethodCall {
            receiver: Box::new(typed_operand(program, *value, ScalarKind::F64, &kinds)),
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
