use super::{ScalarOp, Value, EvalFault, value_from_rat, value_from_exact, EmirValue, exact_int_of, register};

pub(super) fn exact_ratio_arithmetic(
    ln: emath_rt::ExactInt,
    ld: emath_rt::ExactInt,
    rn: emath_rt::ExactInt,
    rd: emath_rt::ExactInt,
    kind: ScalarOp,
    op: &'static str,
) -> Result<Value, EvalFault> {
    let refuse = |err: emath_rt::ExactError| EvalFault::CarrierRefused {
        op,
        detail: err.to_string(),
    };
    let (num, den) = match kind {
        ScalarOp::Add => (
            ln.mul(&rd)
                .and_then(|left| rn.mul(&ld).and_then(|right| left.add(&right)))
                .map_err(refuse)?,
            ld.mul(&rd).map_err(refuse)?,
        ),
        ScalarOp::Sub => (
            ln.mul(&rd)
                .and_then(|left| rn.mul(&ld).and_then(|right| left.sub(&right)))
                .map_err(refuse)?,
            ld.mul(&rd).map_err(refuse)?,
        ),
        ScalarOp::Mul => (ln.mul(&rn).map_err(refuse)?, ld.mul(&rd).map_err(refuse)?),
        ScalarOp::Div => {
            if rn.is_zero() {
                return Err(EvalFault::CarrierRefused {
                    op,
                    detail: "exact division by zero".into(),
                });
            }
            (ln.mul(&rd).map_err(refuse)?, ld.mul(&rn).map_err(refuse)?)
        }
    };
    if den.is_zero() {
        return Err(EvalFault::CarrierRefused {
            op,
            detail: "exact division by zero".into(),
        });
    }
    let value = value_from_rat(num, den);
    if matches!(kind, ScalarOp::Add | ScalarOp::Sub | ScalarOp::Mul)
        && matches!(&value, Value::Rat { den, .. } if *den == 1)
    {
        if let Value::Rat { num, .. } = value {
            return Ok(value_from_exact(emath_rt::ExactInt::from(num)));
        }
    }
    if matches!(kind, ScalarOp::Add | ScalarOp::Sub | ScalarOp::Mul)
        && matches!(&value, Value::ExactRat { den, .. } if den.is_one())
    {
        if let Value::ExactRat { num, .. } = value {
            return Ok(value_from_exact(num));
        }
    }
    Ok(value)
}

pub(super) fn eval_exact_int_call(
    registers: &[Value],
    name: &str,
    args: &[EmirValue],
) -> Result<Value, EvalFault> {
    let fault = |err: emath_rt::ExactError| EvalFault::CarrierRefused {
        op: "exact-int-call",
        detail: err.to_string(),
    };
    let ints = |count: usize| -> Result<Vec<emath_rt::ExactInt>, EvalFault> {
        if args.len() != count {
            return Err(EvalFault::CarrierRefused {
                op: "exact-int-call",
                detail: format!("{name} argument count"),
            });
        }
        args.iter()
            .map(|arg| exact_int_of(register(registers, *arg)?, "exact-int-call"))
            .collect()
    };
    let seq = |arg: EmirValue| -> Result<Vec<emath_rt::ExactInt>, EvalFault> {
        match register(registers, arg)? {
            Value::List(items) => items
                .iter()
                .map(|item| exact_int_of(item, "exact-int-call"))
                .collect(),
            other => exact_int_of(other, "exact-int-call").map(|n| vec![n]),
        }
    };
    let value = match name {
        "int_quot" => {
            let args = ints(2)?;
            args[0].quot(&args[1]).map_err(fault)?
        }
        "int_rem" => {
            let args = ints(2)?;
            args[0].rem_euclid(&args[1]).map_err(fault)?
        }
        "int_pow" => {
            let args = ints(2)?;
            args[0].pow(&args[1]).map_err(fault)?
        }
        "int_sum" => {
            if args.len() != 1 {
                return Err(EvalFault::Arithmetic {
                    op: "exact-int-call",
                    detail: "int_sum argument count",
                });
            }
            emath_rt::exact_int_sum(&seq(args[0])?).map_err(fault)?
        }
        "int_prod" => {
            if args.len() != 1 {
                return Err(EvalFault::Arithmetic {
                    op: "exact-int-call",
                    detail: "int_prod argument count",
                });
            }
            emath_rt::exact_int_prod(&seq(args[0])?).map_err(fault)?
        }
        other => {
            return Err(EvalFault::CarrierRefused {
                op: "exact-int-call",
                detail: format!("unknown exact integer op `{other}`"),
            });
        }
    };
    Ok(value_from_exact(value))
}

