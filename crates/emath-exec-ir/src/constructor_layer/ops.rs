use super::*;
use super::prelude::*;

/// Ordering across numeric carriers. Pure exact pairs cross-multiply; any
/// pair involving Float64 joins EXACTLY through `f64_exact_rat`, so
/// `1 / 2 == 0.5` is true and `3 < 3.5` compares 3 against 7/2 with no
/// rounding. Non-finite Float64 operands fault `non_finite_scalar`.
pub(super) fn cmp_numeric(left: &CValue, right: &CValue) -> Result<std::cmp::Ordering, ConstructorError> {
    let float_rat = |value: &CValue| -> Result<(ExactInt, ExactInt), ConstructorError> {
        match value {
            CValue::Float64(x) => f64_exact_rat(*x)
                .ok_or_else(|| fault("non_finite_scalar", "non-finite Float64 operand")),
            other => as_rat(other.clone()),
        }
    };
    let (ln, ld) = float_rat(left)?;
    let (rn, rd) = float_rat(right)?;
    Ok(ln
        .mul(&rd)
        .map_err(exact_fault)?
        .cmp(&rn.mul(&ld).map_err(exact_fault)?))
}

pub(super) fn neg(value: CValue) -> Result<CValue, ConstructorError> {
    match value {
        CValue::Int(n) => Ok(CValue::Int(n.checked_neg().map_err(exact_fault)?)),
        CValue::Rat { num, den } => CValue::Rat {
            num: num.checked_neg().map_err(exact_fault)?,
            den,
        }
        .canon(),
        CValue::Float64(x) => Ok(CValue::Float64(-x)),
        other => Err(fault("type", format!("cannot negate {other}"))),
    }
}

pub(super) fn apply_unary(op: UnaryOp, value: CValue) -> Result<CValue, ConstructorError> {
    match op {
        UnaryOp::Neg => neg(value),
        UnaryOp::Pos => Ok(value),
        UnaryOp::Not => match value {
            CValue::Bool(b) => Ok(CValue::Bool(!b)),
            _ => Err(fault("type", "not expects Bool")),
        },
    }
}

pub(super) fn index_seq(seq: CValue, i: ExactInt) -> Result<CValue, ConstructorError> {
    match seq {
        CValue::Sequence(items) => {
            let Some(index) = i.to_usize() else {
                return Err(fault("invalid_index", "sequence index out of range"));
            };
            items
                .get(index)
                .cloned()
                .ok_or_else(|| fault("invalid_index", "sequence index out of range"))
        }
        // Buffer reads take the same checked-index surface as
        // sequences (bead emath-84sfr): one lock scope, no copy.
        CValue::Buffer(cell) => {
            let Some(index) = i.to_usize() else {
                return Err(fault("invalid_index", "buffer index out of range"));
            };
            let items = cell
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            items
                .get(index)
                .cloned()
                .ok_or_else(|| fault("invalid_index", "buffer index out of range"))
        }
        _ => Err(fault("type", "index requires a sequence")),
    }
}

pub(super) fn cons_values(head: CValue, tail: CValue) -> Result<CValue, ConstructorError> {
    match tail {
        CValue::Sequence(mut rest) => {
            // Copy-on-write: the copy happens only when the tail is
            // still shared; a private tail is mutated in place.
            std::sync::Arc::make_mut(&mut rest).insert(0, head);
            Ok(CValue::Sequence(rest))
        }
        other => Err(fault(
            "type",
            format!("sequence tail must be a sequence, found {other}"),
        )),
    }
}

pub(super) fn binary(op: BinaryOp, left: CValue, right: CValue) -> Result<CValue, ConstructorError> {
    if matches!(
        op,
        BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div
    ) && (matches!(left, CValue::Float64(_)) || matches!(right, CValue::Float64(_)))
    {
        let l = float_of(&left)?;
        let r = float_of(&right)?;
        let out = match op {
            BinaryOp::Add => l + r,
            BinaryOp::Sub => l - r,
            BinaryOp::Mul => l * r,
            BinaryOp::Div => {
                if r == 0.0 {
                    return Err(fault("division_by_zero", "Float64 division by zero"));
                }
                l / r
            }
            _ => unreachable!(),
        };
        if !out.is_finite() {
            return Err(fault("non_finite_scalar", "non-finite Float64 result"));
        }
        return Ok(CValue::Float64(out));
    }
    match op {
        BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div => {
            let (ln, ld) = as_rat(left)?;
            let (rn, rd) = as_rat(right)?;
            let (num, den) = match op {
                BinaryOp::Add => (
                    ln.mul(&rd)
                        .and_then(|left| rn.mul(&ld).and_then(|right| left.add(&right)))
                        .map_err(exact_fault)?,
                    ld.mul(&rd).map_err(exact_fault)?,
                ),
                BinaryOp::Sub => (
                    ln.mul(&rd)
                        .and_then(|left| rn.mul(&ld).and_then(|right| left.sub(&right)))
                        .map_err(exact_fault)?,
                    ld.mul(&rd).map_err(exact_fault)?,
                ),
                BinaryOp::Mul => (
                    ln.mul(&rn).map_err(exact_fault)?,
                    ld.mul(&rd).map_err(exact_fault)?,
                ),
                BinaryOp::Div => {
                    if rn.is_zero() {
                        return Err(fault("division_by_zero", "exact division by zero"));
                    }
                    (
                        ln.mul(&rd).map_err(exact_fault)?,
                        ld.mul(&rn).map_err(exact_fault)?,
                    )
                }
                _ => unreachable!(),
            };
            if den.is_zero() {
                return Err(fault("division_by_zero", "exact division by zero"));
            }
            let value = CValue::Rat { num, den }.canon()?;
            if let CValue::Rat { num, den } = &value {
                if den.is_one() && matches!(op, BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul) {
                    return Ok(CValue::Int(num.clone()));
                }
            }
            Ok(value)
        }
        BinaryOp::Eq => Ok(CValue::Bool(eq_values(&left, &right)?)),
        BinaryOp::Ne => Ok(CValue::Bool(!eq_values(&left, &right)?)),
        BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => {
            // Ordering is a checked carrier operation on every numeric
            // carrier: `as_rat` alone refused Float64, so `(1.0) > (2.0)`
            // faulted even though the reference ships it. `cmp_numeric`
            // orders pure exact pairs by cross-multiplication and joins
            // Float64 exactly (a finite binary64 IS an exact rational).
            let cmp = cmp_numeric(&left, &right)?;
            Ok(CValue::Bool(match op {
                BinaryOp::Lt => cmp.is_lt(),
                BinaryOp::Le => cmp.is_le(),
                BinaryOp::Gt => cmp.is_gt(),
                BinaryOp::Ge => cmp.is_ge(),
                _ => false,
            }))
        }
        BinaryOp::And => match (left, right) {
            (CValue::Bool(a), CValue::Bool(b)) => Ok(CValue::Bool(a && b)),
            _ => Err(fault("type", "and expects Bool")),
        },
        BinaryOp::Or => match (left, right) {
            (CValue::Bool(a), CValue::Bool(b)) => Ok(CValue::Bool(a && b || a || b)),
            _ => Err(fault("type", "or expects Bool")),
        },
        BinaryOp::In => match right {
            CValue::Sequence(xs) => {
                let mut found = false;
                for item in xs.iter() {
                    if eq_values(item, &left)? {
                        found = true;
                        break;
                    }
                }
                Ok(CValue::Bool(found))
            }
            _ => Err(fault("type", "`in` expects a sequence")),
        },
        other => Err(fault(
            "implementation_unavailable",
            format!("operator {other:?} is not a scalar carrier operation"),
        )),
    }
}

pub(super) fn float_of(value: &CValue) -> Result<f64, ConstructorError> {
    match value {
        CValue::Float64(x) => Ok(*x),
        CValue::Int(n) => Ok(n.to_f64()),
        CValue::Rat { num, den } => Ok(num.to_f64() / den.to_f64()),
        other => Err(fault("type", format!("not Float64: {other}"))),
    }
}

pub(super) fn eq_values(left: &CValue, right: &CValue) -> Result<bool, ConstructorError> {
    match (left, right) {
        // All numeric pairs compare VALUES, never stored representations:
        // `2 == 2 / 1` and `2 == 2.0` are true, `1 / 2 == 0.5` is true, and
        // the join through Float64 is exact (a finite binary64 is an exact
        // rational). The previous structural default silently returned
        // false for mixed Float64 pairs.
        (
            CValue::Int(_) | CValue::Rat { .. } | CValue::Float64(_),
            CValue::Int(_) | CValue::Rat { .. } | CValue::Float64(_),
        ) => Ok(cmp_numeric(left, right)?.is_eq()),
        (CValue::Bool(a), CValue::Bool(b)) => Ok(a == b),
        // Mutable state has no total value equality: refuse by name
        // (admission refuses statically; this is the runtime backstop
        // for dynamic paths — bead emath-84sfr).
        (CValue::Buffer(_), CValue::Buffer(_)) => Err(fault(
            "buffer_equality_refused",
            "buffer comparison is refused: a mutable carrier has no total value equality",
        )),
        (CValue::Sequence(a), CValue::Sequence(b)) => {
            if a.len() != b.len() {
                return Ok(false);
            }
            for (x, y) in a.iter().zip(b.iter()) {
                if !eq_values(x, y)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        (CValue::Tuple(a), CValue::Tuple(b)) => {
            if a.len() != b.len() {
                return Ok(false);
            }
            for (x, y) in a.iter().zip(b) {
                if !eq_values(x, y)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        (
            CValue::Record {
                type_name: a,
                fields: fa,
            },
            CValue::Record {
                type_name: b,
                fields: fb,
            },
        ) => {
            if a != b || fa.len() != fb.len() {
                return Ok(false);
            }
            for (name, value) in fa.iter() {
                match fb.get(name) {
                    Some(other) if eq_values(value, other)? => {}
                    _ => return Ok(false),
                }
            }
            Ok(true)
        }
        (
            CValue::Variant {
                type_name: a,
                tag: ta,
                fields: fa,
            },
            CValue::Variant {
                type_name: b,
                tag: tb,
                fields: fb,
            },
        ) => {
            if a != b || ta != tb || fa.len() != fb.len() {
                return Ok(false);
            }
            for (x, y) in fa.iter().zip(fb) {
                if !eq_values(x, y)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        _ => Ok(left == right),
    }
}

pub(super) fn tuple_index(name: &str) -> Option<usize> {
    name.strip_prefix('_')?.parse().ok()
}

