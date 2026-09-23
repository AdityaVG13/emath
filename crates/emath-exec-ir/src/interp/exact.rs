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
        "int_root" => {
            let args = ints(2)?;
            args[0].floor_root(&args[1]).map_err(fault)?
        }
        "int_gcd" => {
            let args = ints(2)?;
            emath_rt::ExactInt::gcd(&args[0], &args[1]).map_err(fault)?
        }
        "int_egcd" => {
            let args = ints(2)?;
            let (g, s, t) = emath_rt::ExactInt::egcd(&args[0], &args[1]).map_err(fault)?;
            return Ok(Value::List(vec![
                value_from_exact(g),
                value_from_exact(s),
                value_from_exact(t),
            ]));
        }
        "int_binom" => {
            let args = ints(2)?;
            args[0].binomial(&args[1]).map_err(fault)?
        }
        "int_fact" => ints(1)?[0].factorial().map_err(fault)?,
        "int_double_fact" => ints(1)?[0].double_factorial().map_err(fault)?,
        "int_totient" => ints(1)?[0].totient().map_err(fault)?,
        "int_modinv" => {
            let args = ints(2)?;
            args[0].mod_inv(&args[1]).map_err(fault)?
        }
        "int_sqrt_mod" => {
            let args = ints(2)?;
            args[0].sqrt_mod(&args[1]).map_err(fault)?
        }
        "int_rising" => {
            let args = ints(2)?;
            args[0].rising(&args[1]).map_err(fault)?
        }
        "int_falling" => {
            let args = ints(2)?;
            args[0].falling(&args[1]).map_err(fault)?
        }
        "int_powmod" => {
            let args = ints(3)?;
            args[0].pow_mod(&args[1], &args[2]).map_err(fault)?
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
        "int_sum_from" => {
            if args.len() != 2 {
                return Err(EvalFault::CarrierRefused {
                    op: "exact-int-call",
                    detail: "int_sum_from argument count".into(),
                });
            }
            emath_rt::exact_int_sum_from(
                &seq(args[0])?,
                &exact_int_of(register(registers, args[1])?, "exact-int-call")?,
            )
            .map_err(fault)?
        }
        "int_prod_from" => {
            if args.len() != 2 {
                return Err(EvalFault::CarrierRefused {
                    op: "exact-int-call",
                    detail: "int_prod_from argument count".into(),
                });
            }
            emath_rt::exact_int_prod_from(
                &seq(args[0])?,
                &exact_int_of(register(registers, args[1])?, "exact-int-call")?,
            )
            .map_err(fault)?
        }
        "int_hamming" => {
            if args.len() != 2 {
                return Err(EvalFault::Arithmetic {
                    op: "exact-int-call",
                    detail: "int_hamming argument count",
                });
            }
            emath_rt::exact_int_hamming(&seq(args[0])?, &seq(args[1])?).map_err(fault)?
        }
        "int_weighted_prod" => {
            if args.len() != 4 {
                return Err(EvalFault::CarrierRefused {
                    op: "exact-int-call",
                    detail: "int_weighted_prod argument count".into(),
                });
            }
            emath_rt::exact_int_weighted_prod(
                &seq(args[0])?,
                &seq(args[1])?,
                &exact_int_of(register(registers, args[2])?, "exact-int-call")?,
                &exact_int_of(register(registers, args[3])?, "exact-int-call")?,
            )
            .map_err(fault)?
        }
        "int_poly_eval" => {
            if args.len() != 3 {
                return Err(EvalFault::Arithmetic {
                    op: "exact-int-call",
                    detail: "int_poly_eval argument count",
                });
            }
            emath_rt::exact_int_poly_eval(
                &seq(args[0])?,
                &exact_int_of(register(registers, args[1])?, "exact-int-call")?,
                &exact_int_of(register(registers, args[2])?, "exact-int-call")?,
            )
            .map_err(fault)?
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

