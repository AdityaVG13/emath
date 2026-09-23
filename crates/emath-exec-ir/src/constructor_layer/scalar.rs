use super::{ConstructorError, CValue, ExactInt, Expr, ExprKind, BinaryOp, BTreeSet, Arc, BTreeMap, ExactError};

pub(super) fn fault(code: &str, message: impl Into<String>) -> ConstructorError {
    ConstructorError {
        code: code.into(),
        message: message.into(),
    }
}

pub(super) fn parse_int(text: &str) -> Result<CValue, ConstructorError> {
    parse_exact(text).map(CValue::Int)
}

pub(super) fn parse_exact(text: &str) -> Result<ExactInt, ConstructorError> {
    ExactInt::parse(text).map_err(|_| fault("invalid_literal", format!("not Int: {text}")))
}

/// A bare decimal literal under a `Rat` annotation denotes its exact
/// decimal rational (reference: types chapter, carrier selection).
/// Convert unsuffixed decimal text to an integer pair: `1.5` -> (15, 10),
/// `1.5e+2` -> (150, 1). Suffixed literals (`f16`/`bf16`/`f32`/`f64`/
/// `f128`) keep their explicit floating carrier, and exponents beyond
/// the scale bound return `None` — either way the literal stays `Float64`
/// and the output check refuses the `Rat` annotation honestly instead of
/// silently coercing.
pub(super) fn decimal_to_rational(text: &str) -> Option<(String, String)> {
    const SUFFIXES: [&str; 5] = ["f16", "bf16", "f32", "f64", "f128"];
    if SUFFIXES.iter().any(|suffix| text.ends_with(suffix)) {
        return None;
    }
    let (mantissa, exponent) = match text.find(['e', 'E']) {
        Some(at) => (&text[..at], text[at + 1..].parse::<i64>().ok()?),
        None => (text, 0),
    };
    if exponent.unsigned_abs() > 1000 {
        return None;
    }
    let (int_part, frac_part) = match mantissa.find('.') {
        Some(at) => (&mantissa[..at], &mantissa[at + 1..]),
        None => (mantissa, ""),
    };
    if int_part.is_empty()
        || !int_part.bytes().chain(frac_part.bytes()).all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let num = ExactInt::parse(&format!("{int_part}{frac_part}")).ok()?;
    let pow10 = |n: u32| -> Option<ExactInt> {
        let mut acc = ExactInt::from(1_i128);
        let ten = ExactInt::from(10_i128);
        for _ in 0..n {
            acc = acc.mul(&ten).ok()?;
        }
        Some(acc)
    };
    // value = num * 10^exponent / 10^frac_len, kept as one integer pair.
    let frac = i64::try_from(frac_part.len()).ok()?;
    let shift = exponent.checked_sub(frac)?;
    let (num, den) = if shift >= 0 {
        (
            num.mul(&pow10(u32::try_from(shift).ok()?)?).ok()?,
            ExactInt::from(1_i128),
        )
    } else {
        (num, pow10(u32::try_from(-shift).ok()?)?)
    };
    Some((num.to_string(), den.to_string()))
}

/// Rewrite the numeric spine of an expression so bare decimals become
/// exact rationals: propagated through arithmetic (`+ - * /`), unary
/// operators, and `if` branches. Calls, binders, quotations, and suffixed
/// literals govern their own carriers and stay untouched.
pub(super) fn exact_decimal_spine(expr: &Expr) -> Expr {
    match &expr.kind {
        ExprKind::Float(text) => match decimal_to_rational(text) {
            Some((numer, denom)) => Expr {
                kind: ExprKind::Rational { numer, denom },
                source: expr.source,
            },
            None => expr.clone(),
        },
        ExprKind::Unary { op, value } => Expr {
            kind: ExprKind::Unary {
                op: *op,
                value: Box::new(exact_decimal_spine(value)),
            },
            source: expr.source,
        },
        ExprKind::Binary { op, left, right }
            if matches!(
                op,
                BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div
            ) =>
        {
            Expr {
                kind: ExprKind::Binary {
                    op: *op,
                    left: Box::new(exact_decimal_spine(left)),
                    right: Box::new(exact_decimal_spine(right)),
                },
                source: expr.source,
            }
        }
        ExprKind::If {
            condition,
            then_value,
            else_value,
        } => Expr {
            kind: ExprKind::If {
                condition: condition.clone(),
                then_value: Box::new(exact_decimal_spine(then_value)),
                else_value: Box::new(exact_decimal_spine(else_value)),
            },
            source: expr.source,
        },
        _ => expr.clone(),
    }
}

/// A `given` value for a `Rat`-annotated input binds its exact decimal
/// rational when it is a bare decimal (reference: types chapter).
pub(super) fn bind_exact_given(name: &str, value: &Expr, rat_inputs: &BTreeSet<String>) -> Expr {
    if rat_inputs.contains(name) {
        exact_decimal_spine(value)
    } else {
        value.clone()
    }
}

/// Parse a `--set` / eval value. Scalars: Bool, Int, Rat (gcd-reduced;
/// `1/0` is not a Rat), Float64. Sequences: `[1, 2, 3]` and `[1/2, 3/4]`.
pub fn parse_constructor_scalar(raw: &str) -> CValue {
    let raw = raw.trim();
    if let Some(items) = parse_constructor_sequence(raw) {
        return CValue::Sequence(std::sync::Arc::new(items));
    }
    if raw == "true" {
        return CValue::Bool(true);
    }
    if raw == "false" {
        return CValue::Bool(false);
    }
    if let Some((num, den)) = raw.split_once('/') {
        if let (Ok(n), Ok(d)) = (ExactInt::parse(num.trim()), ExactInt::parse(den.trim())) {
            if let Ok(value) = (CValue::Rat { num: n, den: d }).canon() {
                return value;
            }
        }
    }
    if let Ok(n) = ExactInt::parse(raw) {
        return CValue::Int(n);
    }
    if let Ok(x) = raw.parse::<f64>() {
        return CValue::Float64(x);
    }
    CValue::Record {
        type_name: raw.into(),
        fields: Arc::new(BTreeMap::new()),
    }
}

pub(super) fn parse_constructor_sequence(raw: &str) -> Option<Vec<CValue>> {
    if !raw.starts_with('[') || !raw.ends_with(']') || raw.len() < 2 {
        return None;
    }
    let inner = raw[1..raw.len() - 1].trim();
    if inner.is_empty() {
        return Some(Vec::new());
    }
    let mut items = Vec::new();
    let mut start = 0;
    let mut depth = 0i32;
    for (index, ch) in inner.char_indices() {
        match ch {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth < 0 {
                    return None;
                }
            }
            ',' if depth == 0 => {
                let item = inner[start..index].trim();
                if item.is_empty() {
                    return None;
                }
                items.push(parse_constructor_scalar(item));
                start = index + 1;
            }
            _ => {}
        }
    }
    if depth != 0 {
        return None;
    }
    let last = inner[start..].trim();
    if last.is_empty() {
        return None;
    }
    items.push(parse_constructor_scalar(last));
    Some(items)
}

/// Build a reduced Rat. Zero or overflow is a fault.
pub fn exact_rat(num: ExactInt, den: ExactInt) -> Result<CValue, ConstructorError> {
    CValue::Rat { num, den }.canon()
}

pub(super) fn exact_fault(err: ExactError) -> ConstructorError {
    match err {
        ExactError::Overflow => fault(
            "overflow",
            "exact integer exceeded the constructor memory bound",
        ),
        ExactError::DivisionByZero => fault("division_by_zero", "exact division by zero"),
    }
}

pub(super) fn cint(n: impl Into<ExactInt>) -> CValue {
    CValue::Int(n.into())
}

pub(super) fn expect_int(value: CValue, name: &str) -> Result<ExactInt, ConstructorError> {
    match value {
        CValue::Int(n) => Ok(n),
        other => Err(fault("type", format!("`{name}` expects Int, found {other}"))),
    }
}

pub(super) fn expect_ints(value: CValue, name: &str) -> Result<Vec<ExactInt>, ConstructorError> {
    match value {
        CValue::Sequence(items) => items
            .iter()
            .cloned()
            .map(|item| expect_int(item, name))
            .collect(),
        other => Err(fault(
            "type",
            format!("`{name}` expects sequence(Int), found {other}"),
        )),
    }
}

pub(crate) fn machine_int_basename(name: &str) -> Option<&str> {
    let base = name.rsplit('.').next().unwrap_or(name);
    match base {
        "int_quot" | "int_rem" | "int_root" | "int_gcd" | "int_fact"
        | "int_powmod" | "int_pow" | "int_sum" | "int_prod" | "int_sum_from" | "int_prod_from"
        |         "int_hamming" | "int_poly_eval" | "int_rising" | "int_falling"
        | "int_double_fact" | "int_weighted_prod" | "int_binom" | "int_egcd" | "int_totient" => {
            Some(base)
        }
        _ => None,
    }
}

/// Machine buffer-carrier ops:
/// a separate family from the exact-int ops so the emission lane can
/// fence them independently (they run in the constructor VM only).
pub(crate) fn machine_buffer_basename(name: &str) -> Option<&str> {
    let base = name.rsplit('.').next().unwrap_or(name);
    match base {
        "buffer" | "buffer_set" => Some(base),
        _ => None,
    }
}

pub(super) fn as_rat(value: CValue) -> Result<(ExactInt, ExactInt), ConstructorError> {
    match value {
        CValue::Int(n) => Ok((n, ExactInt::one())),
        CValue::Rat { num, den } => Ok((num, den)),
        other => Err(fault("type", format!("expected exact scalar, found {other}"))),
    }
}

/// Exact rational value of a finite Float64: every finite binary64 is
/// mantissa * 2^exponent with an odd-part mantissa below 2^53, so the
/// join to exact arithmetic never rounds. Non-finite values are `None`
/// — the carrier contract faults them instead of ordering them.
pub(super) fn f64_exact_rat(x: f64) -> Option<(ExactInt, ExactInt)> {
    if !x.is_finite() {
        return None;
    }
    let bits = x.to_bits();
    let negative = bits >> 63 == 1;
    let biased = ((bits >> 52) & 0x7ff) as i32;
    let mantissa_field = bits & 0x000f_ffff_ffff_ffff;
    let (mantissa, exponent) = if biased == 0 {
        (mantissa_field, -1074i32)
    } else {
        (mantissa_field | (1u64 << 52), biased - 1075)
    };
    // 2^k for k in [-1074, 971]: far inside the exact-int limb budget, so
    // the power-of-two products cannot fail; square-and-multiply keeps the
    // construction at ~10 bigint multiplies.
    let pow2 = |mut power: u32| -> ExactInt {
        let mut result = ExactInt::one();
        let mut base = ExactInt::from(2u64);
        while power > 0 {
            if power & 1 == 1 {
                result = result.mul(&base).expect("power of two fits the limb budget");
            }
            power >>= 1;
            if power > 0 {
                base = base.mul(&base).expect("power of two fits the limb budget");
            }
        }
        result
    };
    let (num, den) = if exponent >= 0 {
        (
            ExactInt::from(mantissa)
                .mul(&pow2(exponent as u32))
                .expect("power of two fits the limb budget"),
            ExactInt::one(),
        )
    } else {
        (ExactInt::from(mantissa), pow2((-exponent) as u32))
    };
    let num = if negative {
        num.checked_neg().expect("mantissa negation cannot overflow")
    } else {
        num
    };
    Some((num, den))
}

