use super::{Value, EmirValue, EvalFault, register, rat_pair, exact_ratio_arithmetic, value_from_exact, value_from_rat, BuiltinId, f64_of, bool_of};

/// Type-preserving scalar addition (control mail 66): two `Value::I64`
/// operands add exactly via `i64::checked_add` and return `Value::I64`;
/// Float64 operands retain the existing strict-f64 behavior; any other
/// carrier mix is a typed confusion — never a silent coercion.
#[derive(Clone, Copy)]
pub(super) enum ScalarOp {
    Add,
    Sub,
    Mul,
    Div,
}

pub(super) fn apply_f64(left: f64, right: f64, kind: ScalarOp) -> f64 {
    match kind {
        ScalarOp::Add => left + right,
        ScalarOp::Sub => left - right,
        ScalarOp::Mul => left * right,
        ScalarOp::Div => left / right,
    }
}

pub(super) fn scalar_arithmetic(
    registers: &[Value],
    left: EmirValue,
    right: EmirValue,
    op: &'static str,
    kind: ScalarOp,
) -> Result<Value, EvalFault> {
    let left_value = register(registers, left)?;
    let right_value = register(registers, right)?;
    if let (Some((ln, ld)), Some((rn, rd))) = (rat_pair(left_value), rat_pair(right_value))
        && (matches!(left_value, Value::ExactInt(_) | Value::ExactRat { .. })
            || matches!(right_value, Value::ExactInt(_) | Value::ExactRat { .. }))
    {
        return exact_ratio_arithmetic(ln, ld, rn, rd, kind, op);
    }
    match (left_value, right_value) {
        (Value::Rat { num: an, den: ad }, Value::Rat { num: bn, den: bd }) => {
            let a = (*an, *ad);
            let b = (*bn, *bd);
            let result = match kind {
                ScalarOp::Add => emath_rt::ratio_add(a, b),
                ScalarOp::Sub => emath_rt::ratio_sub(a, b),
                ScalarOp::Mul => emath_rt::ratio_mul(a, b),
                ScalarOp::Div => emath_rt::ratio_div(a, b),
            };
            result
                .map(|(num, den)| Value::Rat { num, den })
                .map_err(|detail| EvalFault::CarrierRefused { op, detail })
        }
        (Value::I64(left), Value::I64(right)) => {
            let result = match kind {
                ScalarOp::Add => left.checked_add(*right).map(Value::I64),
                ScalarOp::Sub => left.checked_sub(*right).map(Value::I64),
                ScalarOp::Mul => left.checked_mul(*right).map(Value::I64),
                ScalarOp::Div => {
                    return emath_rt::ratio_div((i128::from(*left), 1), (i128::from(*right), 1))
                        .map(|(num, den)| Value::Rat { num, den })
                        .map_err(|detail| EvalFault::CarrierRefused { op, detail });
                }
            };
            match result {
                Some(value) => Ok(value),
                None => exact_ratio_arithmetic(
                    emath_rt::ExactInt::from(*left),
                    emath_rt::ExactInt::one(),
                    emath_rt::ExactInt::from(*right),
                    emath_rt::ExactInt::one(),
                    kind,
                    op,
                ),
            }
        }
        (Value::I64(left), Value::Rat { num, den }) => {
            let a = (i128::from(*left), 1);
            let b = (*num, *den);
            let result = match kind {
                ScalarOp::Add => emath_rt::ratio_add(a, b),
                ScalarOp::Sub => emath_rt::ratio_sub(a, b),
                ScalarOp::Mul => emath_rt::ratio_mul(a, b),
                ScalarOp::Div => emath_rt::ratio_div(a, b),
            };
            result
                .map(|(num, den)| Value::Rat { num, den })
                .map_err(|detail| EvalFault::CarrierRefused { op, detail })
        }
        (Value::Rat { num, den }, Value::I64(right)) => {
            let a = (*num, *den);
            let b = (i128::from(*right), 1);
            let result = match kind {
                ScalarOp::Add => emath_rt::ratio_add(a, b),
                ScalarOp::Sub => emath_rt::ratio_sub(a, b),
                ScalarOp::Mul => emath_rt::ratio_mul(a, b),
                ScalarOp::Div => emath_rt::ratio_div(a, b),
            };
            result
                .map(|(num, den)| Value::Rat { num, den })
                .map_err(|detail| EvalFault::CarrierRefused { op, detail })
        }
        (Value::Complex { .. }, _) | (_, Value::Complex { .. }) => {
            let (a, b) = complex_parts(left_value).ok_or(EvalFault::TypeConfusion {
                register: left.0,
                op,
            })?;
            let (c, d) = complex_parts(right_value).ok_or(EvalFault::TypeConfusion {
                register: right.0,
                op,
            })?;
            let (re, im) = match kind {
                ScalarOp::Add => (a + c, b + d),
                ScalarOp::Sub => (a - c, b - d),
                ScalarOp::Mul => (a * c - b * d, a * d + b * c),
                ScalarOp::Div => {
                    let denominator = c * c + d * d;
                    ((a * c + b * d) / denominator, (b * c - a * d) / denominator)
                }
            };
            Ok(Value::Complex { re, im })
        }
        (Value::I64(left), Value::F64(right)) => {
            Ok(Value::F64(apply_f64(*left as f64, *right, kind)))
        }
        (Value::F64(left), Value::I64(right)) => {
            Ok(Value::F64(apply_f64(*left, *right as f64, kind)))
        }
        (Value::F64(left), Value::F64(right)) => Ok(Value::F64(apply_f64(*left, *right, kind))),
        (Value::Vector(left), Value::Vector(right)) if left.len() == right.len() => {
            Ok(Value::Vector(
                left.iter()
                    .zip(right.iter())
                    .map(|(a, b)| apply_f64(*a, *b, kind))
                    .collect(),
            ))
        }
        (Value::Vector(left), Value::F64(right)) => Ok(Value::Vector(
            left.iter().map(|a| apply_f64(*a, *right, kind)).collect(),
        )),
        (Value::F64(left), Value::Vector(right)) => Ok(Value::Vector(
            right.iter().map(|b| apply_f64(*left, *b, kind)).collect(),
        )),
        (
            Value::Matrix {
                rows,
                cols,
                data: left,
            },
            Value::Matrix {
                rows: right_rows,
                cols: right_cols,
                data: right,
            },
        ) if rows == right_rows && cols == right_cols => Ok(Value::Matrix {
            rows: *rows,
            cols: *cols,
            data: left
                .iter()
                .zip(right.iter())
                .map(|(a, b)| apply_f64(*a, *b, kind))
                .collect(),
        }),
        (Value::Matrix { rows, cols, data }, Value::F64(scalar)) => Ok(Value::Matrix {
            rows: *rows,
            cols: *cols,
            data: data.iter().map(|a| apply_f64(*a, *scalar, kind)).collect(),
        }),
        (Value::F64(scalar), Value::Matrix { rows, cols, data }) => Ok(Value::Matrix {
            rows: *rows,
            cols: *cols,
            data: data.iter().map(|a| apply_f64(*scalar, *a, kind)).collect(),
        }),
        (Value::Tensor { shape, data }, Value::F64(scalar)) => Ok(Value::Tensor {
            shape: shape.clone(),
            data: data.iter().map(|a| apply_f64(*a, *scalar, kind)).collect(),
        }),
        (Value::F64(scalar), Value::Tensor { shape, data }) => Ok(Value::Tensor {
            shape: shape.clone(),
            data: data.iter().map(|a| apply_f64(*scalar, *a, kind)).collect(),
        }),
        (
            Value::Tensor {
                shape: left_shape,
                data: left,
            },
            Value::Tensor {
                shape: right_shape,
                data: right,
            },
        ) if left_shape == right_shape => Ok(Value::Tensor {
            shape: left_shape.clone(),
            data: left
                .iter()
                .zip(right.iter())
                .map(|(a, b)| apply_f64(*a, *b, kind))
                .collect(),
        }),
        _ => Err(EvalFault::TypeConfusion {
            register: left.0,
            op,
        }),
    }
}

pub(super) fn complex_parts(value: &Value) -> Option<(f64, f64)> {
    match value {
        Value::Complex { re, im } => Some((*re, *im)),
        Value::F64(value) => Some((*value, 0.0)),
        Value::I64(value) => Some((*value as f64, 0.0)),
        _ => None,
    }
}

pub(super) fn scalar_neg(registers: &[Value], value: EmirValue) -> Result<Value, EvalFault> {
    match register(registers, value)? {
        Value::I64(value) => match value.checked_neg() {
            Some(n) => Ok(Value::I64(n)),
            None => emath_rt::ExactInt::from(*value)
                .checked_neg()
                .map(value_from_exact)
                .map_err(|err| EvalFault::CarrierRefused {
                    op: "neg",
                    detail: err.to_string(),
                }),
        },
        Value::ExactInt(value) => value
            .checked_neg()
            .map(value_from_exact)
            .map_err(|err| EvalFault::CarrierRefused {
                op: "neg",
                detail: err.to_string(),
            }),
        Value::ExactRat { num, den } => num
            .checked_neg()
            .map(|num| value_from_rat(num, den.clone()))
            .map_err(|err| EvalFault::CarrierRefused {
                op: "neg",
                detail: err.to_string(),
            }),
        Value::Rat { num, den } => emath_rt::ratio_sub((0, 1), (*num, *den))
            .map(|(num, den)| Value::Rat { num, den })
            .map_err(|detail| EvalFault::CarrierRefused { op: "neg", detail }),
        Value::F64(value) => Ok(Value::F64(-value)),
        Value::Complex { re, im } => Ok(Value::Complex { re: -*re, im: -*im }),
        _ => Err(EvalFault::TypeConfusion {
            register: value.0,
            op: "neg",
        }),
    }
}

pub(super) fn scalar_unary(
    registers: &[Value],
    value: EmirValue,
    builtin: BuiltinId,
) -> Result<Value, EvalFault> {
    if let Value::Complex { re, im } = register(registers, value)? {
        let result = match builtin {
            BuiltinId::Abs => return Ok(Value::F64(re.hypot(*im))),
            BuiltinId::Sqrt => emath_rt::complex_sqrt(*re, *im),
            BuiltinId::Ln => emath_rt::complex_ln(*re, *im),
            BuiltinId::Exp => emath_rt::complex_exp(*re, *im),
            BuiltinId::Log2 => {
                let (re, im) = emath_rt::complex_ln(*re, *im);
                (re / std::f64::consts::LN_2, im / std::f64::consts::LN_2)
            }
            BuiltinId::Log10 => {
                let (re, im) = emath_rt::complex_ln(*re, *im);
                (re / std::f64::consts::LN_10, im / std::f64::consts::LN_10)
            }
            _ => {
                return Err(EvalFault::TypeConfusion {
                    register: value.0,
                    op: "unary-kernel",
                });
            }
        };
        return Ok(Value::Complex {
            re: result.0,
            im: result.1,
        });
    }
    builtin
        .eval_unary(f64_of(registers, value, "unary-kernel")?)
        .map(Value::F64)
        .ok_or(EvalFault::Arithmetic {
            op: "unary-kernel",
            detail: "binary scalar opcode used in unary instruction",
        })
}

pub(super) fn scalar_binary(
    registers: &[Value],
    left: EmirValue,
    right: EmirValue,
    op: &'static str,
    evaluate: impl FnOnce(f64, f64) -> f64,
) -> Result<Value, EvalFault> {
    Ok(Value::F64(evaluate(
        f64_of(registers, left, op)?,
        f64_of(registers, right, op)?,
    )))
}

pub(super) fn comparison(
    registers: &[Value],
    left: EmirValue,
    right: EmirValue,
    op: &'static str,
    compare: impl FnOnce(f64, f64) -> bool,
) -> Result<Value, EvalFault> {
    match (register(registers, left)?, register(registers, right)?) {
        (Value::Rat { num: an, den: ad }, Value::Rat { num: bn, den: bd }) => {
            let a = (*an, *ad);
            let b = (*bn, *bd);
            let (left, right, negate) = match op {
                "lt" => (a, b, false),
                "le" => (b, a, true),
                "gt" => (b, a, false),
                "ge" => (a, b, true),
                _ => unreachable!("ordered comparison instruction"),
            };
            return emath_rt::ratio_lt(left, right)
                .map(|less| Value::Bool(less != negate))
                .map_err(|detail| EvalFault::CarrierRefused { op, detail });
        }
        (Value::I64(left), Value::I64(right)) => {
            return Ok(Value::Bool(compare_ordering(left.cmp(right), op)));
        }
        _ if rat_pair(register(registers, left)?).is_some()
            && rat_pair(register(registers, right)?).is_some() =>
        {
            let (ln, ld) = rat_pair(register(registers, left)?).expect("pair");
            let (rn, rd) = rat_pair(register(registers, right)?).expect("pair");
            let cmp = ln
                .mul(&rd)
                .and_then(|left| Ok(left.cmp(&rn.mul(&ld)?)))
                .map_err(|err| EvalFault::CarrierRefused {
                    op,
                    detail: err.to_string(),
                })?;
            return Ok(Value::Bool(compare_ordering(cmp, op)));
        }
        (Value::I64(left), Value::F64(right)) => {
            return Ok(Value::Bool(
                emath_rt::cmp_i64_f64(*left, *right)
                    .is_some_and(|ordering| compare_ordering(ordering, op)),
            ));
        }
        (Value::F64(left), Value::I64(right)) => {
            return Ok(Value::Bool(
                emath_rt::cmp_i64_f64(*right, *left)
                    .map(std::cmp::Ordering::reverse)
                    .is_some_and(|ordering| compare_ordering(ordering, op)),
            ));
        }
        _ => {}
    }
    Ok(Value::Bool(compare(
        f64_of(registers, left, op)?,
        f64_of(registers, right, op)?,
    )))
}

pub(super) fn compare_ordering(ordering: std::cmp::Ordering, op: &'static str) -> bool {
    match op {
        "lt" => ordering.is_lt(),
        "le" => ordering.is_le(),
        "gt" => ordering.is_gt(),
        "ge" => ordering.is_ge(),
        _ => false,
    }
}

pub(super) fn boolean_binary(
    registers: &[Value],
    left: EmirValue,
    right: EmirValue,
    op: &'static str,
    evaluate: impl FnOnce(bool, bool) -> bool,
) -> Result<Value, EvalFault> {
    Ok(Value::Bool(evaluate(
        bool_of(registers, left, op)?,
        bool_of(registers, right, op)?,
    )))
}

