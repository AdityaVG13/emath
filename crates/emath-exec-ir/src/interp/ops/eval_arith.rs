//! Arithmetic, builtin, and boolean op evaluation.

use super::*;

pub(super) fn eval_arith_op(
    op: &EmirOp,
    registers: &[Value],
    name: &'static str,
) -> Result<Value, EvalFault> {
    match *op {
        EmirOp::F64Add(left, right) => {
            let l = register(registers, left)?;
            let r = register(registers, right)?;
            match (l, r) {
                (Value::Interval { .. }, _) | (_, Value::Interval { .. }) => {
                    let (alo, ahi) = interval_of(registers, left, name)?;
                    let (blo, bhi) = interval_of(registers, right, name)?;
                    Ok(Value::Interval {
                        lo: alo + blo,
                        hi: ahi + bhi,
                    })
                }
                (Value::Complex { .. }, _) | (_, Value::Complex { .. }) => {
                    let (lr, li) = complex_of(registers, left, name)?;
                    let (rr, ri) = complex_of(registers, right, name)?;
                    Ok(Value::Complex {
                        re: lr + rr,
                        im: li + ri,
                    })
                }
                (Value::I64(a), Value::I64(b)) => i64_checked(*a, *b, name, i64::checked_add),
                (Value::Rat { .. }, _) | (_, Value::Rat { .. }) => {
                    rat_binary(registers, left, right, name, RatCombine::Add)
                }
                _ => Ok(Value::F64(
                    f64_of(registers, left, name)? + f64_of(registers, right, name)?,
                )),
            }
        }
        EmirOp::F64Sub(left, right) => {
            let l = register(registers, left)?;
            let r = register(registers, right)?;
            match (l, r) {
                (Value::Interval { .. }, _) | (_, Value::Interval { .. }) => {
                    let (alo, ahi) = interval_of(registers, left, name)?;
                    let (blo, bhi) = interval_of(registers, right, name)?;
                    Ok(Value::Interval {
                        lo: alo - bhi,
                        hi: ahi - blo,
                    })
                }
                (Value::Complex { .. }, _) | (_, Value::Complex { .. }) => {
                    let (lr, li) = complex_of(registers, left, name)?;
                    let (rr, ri) = complex_of(registers, right, name)?;
                    Ok(Value::Complex {
                        re: lr - rr,
                        im: li - ri,
                    })
                }
                (Value::I64(a), Value::I64(b)) => i64_checked(*a, *b, name, i64::checked_sub),
                (Value::Rat { .. }, _) | (_, Value::Rat { .. }) => {
                    rat_binary(registers, left, right, name, RatCombine::Sub)
                }
                _ => Ok(Value::F64(
                    f64_of(registers, left, name)? - f64_of(registers, right, name)?,
                )),
            }
        }
        EmirOp::F64Mul(left, right) => {
            let l = register(registers, left)?;
            let r = register(registers, right)?;
            match (l, r) {
                (Value::Interval { .. }, _) | (_, Value::Interval { .. }) => {
                    let (alo, ahi) = interval_of(registers, left, name)?;
                    let (blo, bhi) = interval_of(registers, right, name)?;
                    // Certified propagation: the result bounds enclose the
                    // product over every pair of points in the operands.
                    let products = [alo * blo, alo * bhi, ahi * blo, ahi * bhi];
                    Ok(Value::Interval {
                        lo: products.iter().cloned().fold(products[0], f64::min),
                        hi: products.iter().cloned().fold(products[0], f64::max),
                    })
                }
                (Value::Complex { .. }, _) | (_, Value::Complex { .. }) => {
                    let (lr, li) = complex_of(registers, left, name)?;
                    let (rr, ri) = complex_of(registers, right, name)?;
                    // (a + bi)(c + di) = (ac - bd) + (ad + bc)i
                    Ok(Value::Complex {
                        re: lr * rr - li * ri,
                        im: lr * ri + li * rr,
                    })
                }
                (Value::I64(a), Value::I64(b)) => i64_checked(*a, *b, name, i64::checked_mul),
                (Value::Rat { .. }, _) | (_, Value::Rat { .. }) => {
                    rat_binary(registers, left, right, name, RatCombine::Mul)
                }
                _ => Ok(Value::F64(
                    f64_of(registers, left, name)? * f64_of(registers, right, name)?,
                )),
            }
        }
        EmirOp::F64Div(left, right) => {
            let l = register(registers, left)?;
            let r = register(registers, right)?;
            match (l, r) {
                (Value::Interval { .. }, _) | (_, Value::Interval { .. }) => {
                    let (alo, ahi) = interval_of(registers, left, name)?;
                    let (blo, bhi) = interval_of(registers, right, name)?;
                    // Zero-CONTAINING divisor: typed refusal, never a
                    // silently widened interval.
                    if blo <= 0.0 && 0.0 <= bhi {
                        return Err(EvalFault::Arithmetic {
                            op: name,
                            detail: "interval divisor contains zero",
                        });
                    }
                    // 1/b on a zero-free interval: bounds flip.
                    let products = [
                        alo * (1.0 / bhi),
                        alo * (1.0 / blo),
                        ahi * (1.0 / bhi),
                        ahi * (1.0 / blo),
                    ];
                    Ok(Value::Interval {
                        lo: products.iter().cloned().fold(products[0], f64::min),
                        hi: products.iter().cloned().fold(products[0], f64::max),
                    })
                }
                (Value::Complex { .. }, _) | (_, Value::Complex { .. }) => {
                    let (lr, li) = complex_of(registers, left, name)?;
                    let (rr, ri) = complex_of(registers, right, name)?;
                    let denom = rr * rr + ri * ri;
                    // (a + bi) / (c + di) = ((ac + bd) + (bc - ad)i) / (c² + d²)
                    Ok(Value::Complex {
                        re: (lr * rr + li * ri) / denom,
                        im: (li * rr - lr * ri) / denom,
                    })
                }
                (Value::Rat { .. }, _) | (_, Value::Rat { .. }) => {
                    rat_binary(registers, left, right, name, RatCombine::Div)
                }
                (Value::I64(a), Value::I64(b)) => {
                    if *b == 0 {
                        return Err(EvalFault::Arithmetic {
                            op: name,
                            detail: "integer divisor is zero",
                        });
                    }
                    // Exact-Int lane: only exact quotients compute; an
                    // inexact quotient refuses (never an f64 truncation).
                    if a % b != 0 {
                        return Err(EvalFault::Arithmetic {
                            op: name,
                            detail: "i64 division is inexact; use a Float64 or Rat input",
                        });
                    }
                    i64_checked(*a, *b, name, i64::checked_div)
                }
                _ => Ok(Value::F64(
                    f64_of(registers, left, name)? / f64_of(registers, right, name)?,
                )),
            }
        }
        EmirOp::F64Pow(left, right) => Ok(Value::F64(
            f64_of(registers, left, name)?.powf(f64_of(registers, right, name)?),
        )),
        EmirOp::Neg(value) => match register(registers, value)? {
            Value::Complex { re, im } => Ok(Value::Complex { re: -*re, im: -*im }),
            Value::I64(n) => n
                .checked_neg()
                .map(Value::I64)
                .ok_or(EvalFault::Arithmetic {
                    op: name,
                    detail: "i64 overflow",
                }),
            _ => Ok(Value::F64(-f64_of(registers, value, name)?)),
        },
        EmirOp::UnaryBuiltin(id, value) => match register(registers, value)? {
            Value::Complex { re, im } => eval_complex_unary(id, *re, *im, value.0, name),
            _ => Ok(Value::F64(id.eval_unary(f64_of(registers, value, name)?))),
        },
        EmirOp::BinaryBuiltin(id, left, right) => {
            // Exact-Int special case: `mod` over two Int registers stays
            // exact and refuses mod-zero typed (an Int output can never
            // be NaN); every other builtin and operand mix stays f64.
            if id == BuiltinId::Mod {
                match (register(registers, left)?, register(registers, right)?) {
                    (Value::I64(a), Value::I64(b)) => {
                        if *b == 0 {
                            return Err(EvalFault::Arithmetic {
                                op: name,
                                detail: "integer mod zero",
                            });
                        }
                        Ok(Value::I64(a % b))
                    }
                    _ => Ok(Value::F64(id.eval_binary(
                        f64_of(registers, left, name)?,
                        f64_of(registers, right, name)?,
                    ))),
                }
            } else {
                let l = f64_of(registers, left, name)?;
                let r = f64_of(registers, right, name)?;
                Ok(Value::F64(id.eval_binary(l, r)))
            }
        }
        EmirOp::Lt(left, right) => {
            ord_cmp(registers, left, right, name, |o| o.is_lt(), |a, b| a < b)
        }
        EmirOp::Le(left, right) => {
            ord_cmp(registers, left, right, name, |o| o.is_le(), |a, b| a <= b)
        }
        EmirOp::Gt(left, right) => {
            ord_cmp(registers, left, right, name, |o| o.is_gt(), |a, b| a > b)
        }
        EmirOp::Ge(left, right) => {
            ord_cmp(registers, left, right, name, |o| o.is_ge(), |a, b| a >= b)
        }
        EmirOp::Eq(left, right) => eq_ne(registers, left, right, name, true),
        EmirOp::Ne(left, right) => eq_ne(registers, left, right, name, false),
        EmirOp::And(left, right) => Ok(Value::Bool(
            bool_of(registers, left, name)? && bool_of(registers, right, name)?,
        )),
        EmirOp::Or(left, right) => Ok(Value::Bool(
            bool_of(registers, left, name)? || bool_of(registers, right, name)?,
        )),
        EmirOp::Imply(left, right) => Ok(Value::Bool(
            !bool_of(registers, left, name)? || bool_of(registers, right, name)?,
        )),
        EmirOp::Iff(left, right) => Ok(Value::Bool(
            bool_of(registers, left, name)? == bool_of(registers, right, name)?,
        )),
        EmirOp::Not(value) => Ok(Value::Bool(!bool_of(registers, value, name)?)),
        EmirOp::IsFinite(value) => Ok(Value::Bool(f64_of(registers, value, name)?.is_finite())),
        _ => unreachable!("eval_arith_op routed a non-matching op"),
    }
}
