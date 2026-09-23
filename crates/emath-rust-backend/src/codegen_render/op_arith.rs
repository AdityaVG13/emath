//! Arithmetic, boolean, and comparison op lowering.

use super::{
    BackendError, BinOp, EmirOp, EmirProgram, EmirValue, Expr, UnOp, ValueKind,
    borrowed_register, checked_integer_result, cmp_expr, comparison, exact_int_operand,
    i64_or_f64_bin, kind_at, map_runtime_result, operand, operand_kind, render_expr, to_code_value,
    typed_operand, union_pair,
};
use emath_exec_ir::BuiltinId;

/// Complex binop operand: complex carriers pass through; scalars widen
/// to the tuple carrier exactly as the VM's `complex_parts` does
/// (`(x, 0.0)`); anything else refuses typed.
fn typed_operand_or_complex(program: &EmirProgram, value: EmirValue, kinds: &[ValueKind]) -> Expr {
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

/// Widen an exact-integer operand to the `(i128, i128)` rational carrier
/// (`n` becomes `n/1`, the VM's `as_rat` law). A part beyond `i128`
/// refuses by name - the emitter's declared rational scale boundary -
/// instead of truncating or silently switching representations.
pub(super) fn exact_int_ratio_parts(operand_expr: &str) -> String {
    format!(
        "((({operand_expr}).to_i128().ok_or_else(|| String::from(\"E-RAT-002: exact rational part exceeds i128\"))?), 1)"
    )
}

pub(super) fn op_arith_exprs(
    op: &EmirOp,
    program: &EmirProgram,
    kinds: &[ValueKind],
) -> Result<Expr, BackendError> {
    // The expression-template union lane (bead
    // emath-expression-quotes-324y0): a joinable union pair routes the
    // op onto the dynamic rt kernels, which implement the VM's exact
    // carrier rules. Every render returns the union (comparisons and
    // boolean combinators yield CodeValue::Bool), so the body composes
    // to one union value that the typed boundary projects. The kind
    // rules guarantee the pair is joinable when this lane fires.
    let union_arith = match op {
        EmirOp::F64Add(a, b) => Some(("code_add", *a, *b)),
        EmirOp::F64Sub(a, b) => Some(("code_sub", *a, *b)),
        EmirOp::F64Mul(a, b) => Some(("code_mul", *a, *b)),
        EmirOp::F64Div(a, b) => Some(("code_div", *a, *b)),
        _ => None,
    };
    if let Some((function, left, right)) = union_arith {
        if union_pair(kinds, left, right) {
            let left_kind = kind_at(kinds, left);
            let right_kind = kind_at(kinds, right);
            let left_code = render_expr(&to_code_value(operand(program, left), &left_kind));
            let right_code = render_expr(&to_code_value(operand(program, right), &right_kind));
            return Ok(Expr::Raw(format!(
                "emath_rt::code::{function}(&{left_code}, &{right_code})?"
            )));
        }
    }
    if let EmirOp::Neg(value) = op {
        if kind_at(kinds, *value) == ValueKind::CodeValue {
            return Ok(Expr::Raw(format!(
                "emath_rt::code::code_neg(&{})?",
                render_expr(&operand(program, *value))
            )));
        }
    }
    if let EmirOp::Not(value) = op {
        if kind_at(kinds, *value) == ValueKind::CodeValue {
            return Ok(Expr::Raw(format!(
                "emath_rt::code::CodeValue::Bool(emath_rt::code::code_not(&{})?)",
                render_expr(&operand(program, *value))
            )));
        }
    }
    let union_bool = match op {
        EmirOp::And(a, b) => Some(("&&", *a, *b)),
        EmirOp::Or(a, b) => Some(("||", *a, *b)),
        _ => None,
    };
    if let Some((join, left, right)) = union_bool {
        if union_pair(kinds, left, right) {
            // Short-circuit parity: the VM's And/Or arms evaluate the
            // right side only when the left side does not decide, and
            // Rust's `&&`/`||` over the checked projections do exactly
            // that (a non-Bool operand that is never reached never
            // faults, matching the engine).
            let left_code = render_expr(&operand(program, left));
            let right_code = render_expr(&operand(program, right));
            return Ok(Expr::Raw(format!(
                "emath_rt::code::CodeValue::Bool(emath_rt::code::code_as_bool(&{left_code})? {join} emath_rt::code::code_as_bool(&{right_code})?)"
            )));
        }
    }
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
        if kind_at(kinds, left) == ValueKind::Rational
            && kind_at(kinds, right) == ValueKind::Rational
        {
            let value = map_runtime_result(format!(
                "emath_rt::{function}({}, {})",
                render_expr(&operand(program, left)),
                render_expr(&operand(program, right))
            ));
            return Ok(if negate {
                Expr::Un {
                    op: UnOp::Not,
                    value: Box::new(value),
                }
            } else {
                value
            });
        }
        // Mixed Rational/Float64 (and Int/Float64) arithmetic - the
        // labeled Float64 tier (`hr * 1.0f64`): the exact operand
        // widens through the same as-f64 coercion the VM's
        // Rat-to-Float / Int-to-Float join uses (`numerator as f64 /
        // denominator as f64`, the ratio carrier is normalized so the
        // denominator is positive); the float operand locks the
        // carrier and the result stays F64.
        let float_of = |value: EmirValue| -> Option<Expr> {
            match kind_at(kinds, value) {
                ValueKind::F64 => Some(operand(program, value)),
                ValueKind::I64 => Some(Expr::Raw(format!(
                    "(({}) as f64)",
                    render_expr(&operand(program, value))
                ))),
                ValueKind::Rational => {
                    let ratio = render_expr(&operand(program, value));
                    Some(Expr::Raw(format!(
                        "(((({ratio}).0) as f64) / ((({ratio}).1) as f64))"
                    )))
                }
                _ => None,
            }
        };
        let float_symbol = match function {
            "ratio_add" => Some('+'),
            "ratio_sub" => Some('-'),
            "ratio_mul" => Some('*'),
            "ratio_div" => Some('/'),
            _ => None,
        };
        if let (Some(left_f64), Some(right_f64)) = (float_of(left), float_of(right)) {
            let left_kind = kind_at(kinds, left);
            let right_kind = kind_at(kinds, right);
            // Exactly one float operand and one exact operand: the
            // mixed pair meets on the float carrier. Pure int/int,
            // rat/rat, and float/float pairs take their own lanes.
            let mixed_float = (left_kind == ValueKind::F64
                && matches!(right_kind, ValueKind::Rational | ValueKind::I64))
                || (right_kind == ValueKind::F64
                    && matches!(left_kind, ValueKind::Rational | ValueKind::I64));
            if mixed_float {
                if let Some(symbol) = float_symbol {
                    return Ok(Expr::Raw(format!(
                        "({} {} {})",
                        render_expr(&left_f64),
                        symbol,
                        render_expr(&right_f64)
                    )));
                }
                if function == "ratio_lt" {
                    let value = Expr::Raw(format!(
                        "({} < {})",
                        render_expr(&left_f64),
                        render_expr(&right_f64)
                    ));
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
        }
        // Mixed Int/Rational arithmetic (the residualized
        // expression-template lane hits this: `x * c + 1/2` with Int
        // inputs): the VM's `as_rat` law - the Int operand widens
        // exactly (`n` becomes `n/1`), the result stays Rational (a
        // Rational operand locks the carrier, never a float join).
        // ExactInt-backed operands (e.g. `int_binom` results) widen
        // through the same checked i128 bridge so every mixed pair
        // meets on ONE representation - the ratio tuple carrier -
        // never raw tuple ops against ExactInt values.
        let ratio_of = |value: EmirValue| -> Option<Expr> {
            match kind_at(kinds, value) {
                ValueKind::Rational => Some(operand(program, value)),
                ValueKind::I64 => Some(Expr::Raw(format!(
                    "(i128::from({}), 1)",
                    render_expr(&operand(program, value))
                ))),
                ValueKind::ExactInt => Some(Expr::Raw(exact_int_ratio_parts(&render_expr(
                    &operand(program, value),
                )))),
                _ => None,
            }
        };
        if let (Some(left_ratio), Some(right_ratio)) = (ratio_of(left), ratio_of(right)) {
            if matches!(kind_at(kinds, left), ValueKind::Rational)
                || matches!(kind_at(kinds, right), ValueKind::Rational)
            {
                let value = map_runtime_result(format!(
                    "emath_rt::{function}({}, {})",
                    render_expr(&left_ratio),
                    render_expr(&right_ratio)
                ));
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
        if matches!(kind_at(kinds, left), ValueKind::ExactInt | ValueKind::I64)
            && matches!(kind_at(kinds, right), ValueKind::ExactInt | ValueKind::I64)
            && (kind_at(kinds, left) == ValueKind::ExactInt
                || kind_at(kinds, right) == ValueKind::ExactInt)
        {
            let left_e = render_expr(&exact_int_operand(program, left, kinds));
            let right_e = render_expr(&exact_int_operand(program, right, kinds));
            // Method slots take `&ExactInt`; a borrowed register (the
            // non-copy load lane) already renders as exactly one
            // reference layer, so the `&` is added only for owned
            // operand expressions.
            let right_ref = if kind_at(kinds, right) == ValueKind::ExactInt
                && borrowed_register(program, right, kinds)
            {
                right_e.clone()
            } else {
                format!("&{right_e}")
            };
            let value = match function {
                "ratio_add" => map_runtime_result(format!(
                    "{left_e}.add({right_ref}).map_err(|err| err.to_string())"
                )),
                "ratio_sub" => map_runtime_result(format!(
                    "{left_e}.sub({right_ref}).map_err(|err| err.to_string())"
                )),
                "ratio_mul" => map_runtime_result(format!(
                    "{left_e}.mul({right_ref}).map_err(|err| err.to_string())"
                )),
                // One representation: both operands widen to the ratio
                // tuple carrier. `exact_ratio` returns
                // `(ExactInt, ExactInt)`, which is not the Rational
                // carrier and miscompiles into raw tuple arithmetic.
                "ratio_div" => map_runtime_result(format!(
                    "emath_rt::ratio_div({}, {})",
                    exact_int_ratio_parts(&left_e),
                    exact_int_ratio_parts(&right_e)
                )),
                "ratio_lt" => Expr::Raw(format!(
                    "{left_e}.cmp({right_ref}) == core::cmp::Ordering::Less"
                )),
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
            return Ok(map_runtime_result(format!(
                "emath_rt::ratio_sub((0, 1), {})",
                render_expr(&operand(program, *value))
            )));
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
        if kind_at(kinds, left) == ValueKind::Complex || kind_at(kinds, right) == ValueKind::Complex
        {
            return Ok(Expr::Raw(format!(
                "emath_rt::{function}({}, {})",
                render_expr(&typed_operand_or_complex(program, left, kinds)),
                render_expr(&typed_operand_or_complex(program, right, kinds))
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
            kinds,
        )),
        EmirOp::F64Sub(l, r) => Ok(i64_or_f64_bin(
            BinOp::Sub,
            "checked_sub",
            program,
            *l,
            *r,
            kinds,
        )),
        EmirOp::F64Mul(l, r) => Ok(i64_or_f64_bin(
            BinOp::Mul,
            "checked_mul",
            program,
            *l,
            *r,
            kinds,
        )),
        EmirOp::F64Div(l, r) => {
            if matches!(kind_at(kinds, *l), ValueKind::I64 | ValueKind::ExactInt)
                && matches!(kind_at(kinds, *r), ValueKind::I64 | ValueKind::ExactInt)
                && (kind_at(kinds, *l) == ValueKind::ExactInt
                    || kind_at(kinds, *r) == ValueKind::ExactInt)
            {
                // Same one-representation rule as the exact lane above:
                // widen both operands to the ratio tuple carrier.
                return Ok(map_runtime_result(format!(
                    "emath_rt::ratio_div({}, {})",
                    exact_int_ratio_parts(&render_expr(&exact_int_operand(program, *l, kinds))),
                    exact_int_ratio_parts(&render_expr(&exact_int_operand(program, *r, kinds)))
                )));
            }
            if kind_at(kinds, *l) == ValueKind::I64 && kind_at(kinds, *r) == ValueKind::I64 {
                return Ok(map_runtime_result(format!(
                    "emath_rt::ratio_div((i128::from({}), 1), (i128::from({}), 1))",
                    render_expr(&operand(program, *l)),
                    render_expr(&operand(program, *r))
                )));
            }
            Ok(Expr::Bin {
                op: BinOp::Div,
                left: Box::new(typed_operand(program, *l, ValueKind::F64, kinds)),
                right: Box::new(typed_operand(program, *r, ValueKind::F64, kinds)),
            })
        }
        EmirOp::F64Pow(l, r) => Ok(Expr::Bin {
            op: BinOp::Pow,
            left: Box::new(typed_operand(program, *l, ValueKind::F64, kinds)),
            right: Box::new(typed_operand(program, *r, ValueKind::F64, kinds)),
        }),
        EmirOp::Neg(value) => {
            if operand_kind(kinds, *value) == ValueKind::I64 {
                Ok(checked_integer_result(Expr::MethodCall {
                    receiver: Box::new(operand(program, *value)),
                    method: "checked_neg".to_string(),
                    args: Vec::new(),
                }))
            } else if operand_kind(kinds, *value) == ValueKind::ExactInt {
                Ok(map_runtime_result(format!(
                    "{}.checked_neg().map_err(|err| err.to_string())",
                    render_expr(&operand(program, *value))
                )))
            } else {
                Ok(Expr::Un {
                    op: UnOp::Neg,
                    value: Box::new(typed_operand(program, *value, ValueKind::F64, kinds)),
                })
            }
        }
        EmirOp::Not(value) => Ok(Expr::Un {
            op: UnOp::Not,
            value: Box::new(operand(program, *value)),
        }),
        EmirOp::UnaryBuiltin(id, value) => {
            let arg = render_expr(&typed_operand(program, *value, ValueKind::F64, kinds));
            Ok(Expr::Raw(unary_builtin(*id, &arg)?))
        }
        EmirOp::BinaryBuiltin(id, left, right) => {
            let left = render_expr(&typed_operand(program, *left, ValueKind::F64, kinds));
            let right = render_expr(&typed_operand(program, *right, ValueKind::F64, kinds));
            Ok(Expr::Raw(binary_builtin(*id, &left, &right)?))
        }
        EmirOp::IsFinite(value) => Ok(Expr::MethodCall {
            receiver: Box::new(typed_operand(program, *value, ValueKind::F64, kinds)),
            method: "is_finite".to_string(),
            args: Vec::new(),
        }),
        EmirOp::Lt(l, r) => Ok(cmp_expr(BinOp::Lt, program, *l, *r, kinds)),
        EmirOp::Le(l, r) => Ok(cmp_expr(BinOp::Le, program, *l, *r, kinds)),
        EmirOp::Gt(l, r) => Ok(cmp_expr(BinOp::Gt, program, *l, *r, kinds)),
        EmirOp::Ge(l, r) => Ok(cmp_expr(BinOp::Ge, program, *l, *r, kinds)),
        EmirOp::Eq(l, r) => Ok(cmp_expr(BinOp::Eq, program, *l, *r, kinds)),
        EmirOp::Ne(l, r) => Ok(cmp_expr(BinOp::Ne, program, *l, *r, kinds)),
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
