// Exact kernels use the existing (numerator, denominator) representation.
// Every public result has a positive, gcd-reduced denominator. Checked i128
// intermediates can refuse; no operation converts an exact value to a float.
pub type ExactRatio = (i128, i128);
type ExactResult<T> = Result<T, String>;
fn q_overflow() -> String {
    "E-RAT-002: exact rational intermediate exceeds i128".into()
}
fn q_gcd(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}
pub fn ratio_normalize(n: i128, d: i128) -> ExactResult<ExactRatio> {
    if d == 0 {
        return Err("E-RAT-001: denominator is zero".into());
    }
    let g = q_gcd(n.unsigned_abs(), d.unsigned_abs());
    let magnitude = n.unsigned_abs() / g;
    let denominator = i128::try_from(d.unsigned_abs() / g).map_err(|_| q_overflow())?;
    let negative = (n < 0) != (d < 0);
    let numerator = if negative && magnitude == (1_u128 << 127) {
        i128::MIN
    } else {
        let value = i128::try_from(magnitude).map_err(|_| q_overflow())?;
        if negative { -value } else { value }
    };
    Ok((numerator, denominator))
}
pub fn ratio_construct(n: i64, d: i64) -> ExactResult<ExactRatio> {
    ratio_normalize(i128::from(n), i128::from(d))
}
pub fn ratio_norm(q: ExactRatio) -> ExactResult<ExactRatio> {
    ratio_normalize(q.0, q.1)
}
fn q_valid(q: ExactRatio) -> bool {
    ratio_norm(q).is_ok_and(|canonical| canonical == q)
}
fn q_require(q: ExactRatio) -> ExactResult<()> {
    if q_valid(q) {
        Ok(())
    } else {
        Err("E-RAT-001: noncanonical rational".into())
    }
}
pub fn ratio_add(a: ExactRatio, b: ExactRatio) -> ExactResult<ExactRatio> {
    q_sum(a, b, false)
}
pub fn ratio_sub(a: ExactRatio, b: ExactRatio) -> ExactResult<ExactRatio> {
    q_sum(a, b, true)
}
fn q_sum(a: ExactRatio, b: ExactRatio, subtract: bool) -> ExactResult<ExactRatio> {
    q_require(a)?;
    q_require(b)?;
    let g = q_gcd(a.1 as u128, b.1 as u128) as i128;
    let left = a.0.checked_mul(b.1 / g).ok_or_else(q_overflow)?;
    let right = b.0.checked_mul(a.1 / g).ok_or_else(q_overflow)?;
    let n = if subtract {
        left.checked_sub(right)
    } else {
        left.checked_add(right)
    }
    .ok_or_else(q_overflow)?;
    let h = q_gcd(n.unsigned_abs(), g as u128) as i128;
    ratio_normalize(
        n / h,
        (a.1 / g).checked_mul(b.1 / h).ok_or_else(q_overflow)?,
    )
}
pub fn ratio_mul(a: ExactRatio, b: ExactRatio) -> ExactResult<ExactRatio> {
    q_require(a)?;
    q_require(b)?;
    let g = q_gcd(a.0.unsigned_abs(), b.1 as u128) as i128;
    let h = q_gcd(b.0.unsigned_abs(), a.1 as u128) as i128;
    ratio_normalize(
        (a.0 / g).checked_mul(b.0 / h).ok_or_else(q_overflow)?,
        (a.1 / h).checked_mul(b.1 / g).ok_or_else(q_overflow)?,
    )
}
pub fn ratio_div(a: ExactRatio, b: ExactRatio) -> ExactResult<ExactRatio> {
    q_require(a)?;
    q_require(b)?;
    if b.0 == 0 {
        return Err("E-RAT-001: division by zero".into());
    }
    // Reduce unsigned numerator magnitudes before forming the reciprocal. This
    // permits MIN/MIN even though the positive reciprocal denominator cannot fit.
    let g = q_gcd(a.0.unsigned_abs(), b.0.unsigned_abs());
    let h = q_gcd(a.1 as u128, b.1 as u128) as i128;
    let divide_magnitude = |n: i128| -> ExactResult<i128> {
        let magnitude = n.unsigned_abs() / g;
        if n < 0 && magnitude == (1_u128 << 127) {
            Ok(i128::MIN)
        } else {
            let v = i128::try_from(magnitude).map_err(|_| q_overflow())?;
            Ok(if n < 0 { -v } else { v })
        }
    };
    ratio_normalize(
        divide_magnitude(a.0)?
            .checked_mul(b.1 / h)
            .ok_or_else(q_overflow)?,
        (a.1 / h)
            .checked_mul(divide_magnitude(b.0)?)
            .ok_or_else(q_overflow)?,
    )
}
fn q_cmp(a: ExactRatio, b: ExactRatio) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    if (a.0 < 0) != (b.0 < 0) {
        return a.0.cmp(&b.0);
    }
    let (mut n, mut d, mut p, mut q) = (
        a.0.unsigned_abs(),
        a.1 as u128,
        b.0.unsigned_abs(),
        b.1 as u128,
    );
    let mut reverse = a.0 < 0;
    loop {
        let order = (n / d).cmp(&(p / q));
        if order != Ordering::Equal {
            return if reverse { order.reverse() } else { order };
        }
        let (r, s) = (n % d, p % q);
        if r == 0 || s == 0 {
            let order = r.cmp(&s);
            return if reverse { order.reverse() } else { order };
        }
        (n, d, p, q) = (d, r, q, s);
        reverse = !reverse;
    }
}
pub fn ratio_lt(a: ExactRatio, b: ExactRatio) -> ExactResult<bool> {
    q_require(a)?;
    q_require(b)?;
    Ok(q_cmp(a, b).is_lt())
}
