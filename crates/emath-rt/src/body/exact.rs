// Parts share the reference engine's exact integer representation and memory
// bound. Every kernel result has a positive, gcd-reduced denominator.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExactRatio(pub ExactInt, pub ExactInt);

impl<N: Into<ExactInt>, D: Into<ExactInt>> From<(N, D)> for ExactRatio {
    fn from((numer, denom): (N, D)) -> Self {
        Self(numer.into(), denom.into())
    }
}

impl From<&ExactRatio> for ExactRatio {
    fn from(value: &ExactRatio) -> Self {
        value.clone()
    }
}

impl<N: Clone + Into<ExactInt>, D: Clone + Into<ExactInt>> PartialEq<(N, D)> for ExactRatio {
    fn eq(&self, other: &(N, D)) -> bool {
        self.0 == other.0.clone().into() && self.1 == other.1.clone().into()
    }
}

type ExactResult<T> = Result<T, String>;
fn q_error(error: ExactError) -> String {
    match error {
        ExactError::DivisionByZero => "E-RAT-001: denominator is zero".into(),
        ExactError::Overflow => "overflow: exact rational exceeded the integer memory bound".into(),
    }
}
fn q_gcd(a: &ExactInt, b: &ExactInt) -> ExactResult<ExactInt> {
    ExactInt::gcd(a, b).map_err(q_error)
}
pub fn ratio_normalize(n: impl Into<ExactInt>, d: impl Into<ExactInt>) -> ExactResult<ExactRatio> {
    let (n, d) = exact_ratio(n.into(), d.into()).map_err(q_error)?;
    Ok(ExactRatio(n, d))
}
pub fn ratio_construct(n: i64, d: i64) -> ExactResult<ExactRatio> {
    ratio_normalize(n, d)
}
pub fn ratio_norm(q: impl Into<ExactRatio>) -> ExactResult<ExactRatio> {
    let q = q.into();
    ratio_normalize(q.0, q.1)
}
fn q_require(q: &ExactRatio) -> ExactResult<()> {
    if q.1.signum() <= 0 || !q_gcd(&q.0, &q.1)?.is_one() {
        return Err("E-RAT-001: noncanonical rational".into());
    }
    Ok(())
}
pub fn ratio_add(a: impl Into<ExactRatio>, b: impl Into<ExactRatio>) -> ExactResult<ExactRatio> {
    q_sum(a.into(), b.into(), false)
}
pub fn ratio_sub(a: impl Into<ExactRatio>, b: impl Into<ExactRatio>) -> ExactResult<ExactRatio> {
    q_sum(a.into(), b.into(), true)
}
fn q_sum(a: ExactRatio, b: ExactRatio, subtract: bool) -> ExactResult<ExactRatio> {
    q_require(&a)?;
    q_require(&b)?;
    let g = q_gcd(&a.1, &b.1)?;
    let ad = a.1.quot(&g).map_err(q_error)?;
    let bd = b.1.quot(&g).map_err(q_error)?;
    let left = a.0.mul(&bd).map_err(q_error)?;
    let right = b.0.mul(&ad).map_err(q_error)?;
    let n = if subtract {
        left.sub(&right)
    } else {
        left.add(&right)
    }
    .map_err(q_error)?;
    let h = q_gcd(&n, &g)?;
    ratio_normalize(
        n.quot(&h).map_err(q_error)?,
        ad.mul(&b.1.quot(&h).map_err(q_error)?).map_err(q_error)?,
    )
}
pub fn ratio_mul(a: impl Into<ExactRatio>, b: impl Into<ExactRatio>) -> ExactResult<ExactRatio> {
    let (a, b) = (a.into(), b.into());
    q_require(&a)?;
    q_require(&b)?;
    let g = q_gcd(&a.0, &b.1)?;
    let h = q_gcd(&b.0, &a.1)?;
    ratio_normalize(
        a.0.quot(&g)
            .map_err(q_error)?
            .mul(&b.0.quot(&h).map_err(q_error)?)
            .map_err(q_error)?,
        a.1.quot(&h)
            .map_err(q_error)?
            .mul(&b.1.quot(&g).map_err(q_error)?)
            .map_err(q_error)?,
    )
}
pub fn ratio_div(a: impl Into<ExactRatio>, b: impl Into<ExactRatio>) -> ExactResult<ExactRatio> {
    let (a, b) = (a.into(), b.into());
    q_require(&a)?;
    q_require(&b)?;
    if b.0.is_zero() {
        return Err("E-RAT-001: division by zero".into());
    }
    let g = q_gcd(&a.0, &b.0)?;
    let h = q_gcd(&a.1, &b.1)?;
    ratio_normalize(
        a.0.quot(&g)
            .map_err(q_error)?
            .mul(&b.1.quot(&h).map_err(q_error)?)
            .map_err(q_error)?,
        a.1.quot(&h)
            .map_err(q_error)?
            .mul(&b.0.quot(&g).map_err(q_error)?)
            .map_err(q_error)?,
    )
}
fn q_cmp(a: ExactRatio, b: ExactRatio) -> ExactResult<std::cmp::Ordering> {
    use std::cmp::Ordering;
    if a.0.signum() != b.0.signum() {
        return Ok(a.0.signum().cmp(&b.0.signum()));
    }
    let (mut n, mut d, mut p, mut q) = (a.0.abs(), a.1, b.0.abs(), b.1);
    let mut reverse = a.0.is_negative();
    loop {
        let (left, r) = n.div_rem(&d).map_err(q_error)?;
        let (right, s) = p.div_rem(&q).map_err(q_error)?;
        let order = left.cmp(&right);
        if order != Ordering::Equal {
            return Ok(if reverse { order.reverse() } else { order });
        }
        if r.is_zero() || s.is_zero() {
            let order = r.cmp(&s);
            return Ok(if reverse { order.reverse() } else { order });
        }
        (n, d, p, q) = (d, r, q, s);
        reverse = !reverse;
    }
}
pub fn ratio_cmp(
    a: impl Into<ExactRatio>,
    b: impl Into<ExactRatio>,
) -> ExactResult<std::cmp::Ordering> {
    let (a, b) = (a.into(), b.into());
    q_require(&a)?;
    q_require(&b)?;
    q_cmp(a, b)
}
pub fn ratio_lt(a: impl Into<ExactRatio>, b: impl Into<ExactRatio>) -> ExactResult<bool> {
    Ok(ratio_cmp(a, b)?.is_lt())
}
