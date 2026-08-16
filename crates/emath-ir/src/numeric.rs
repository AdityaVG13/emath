//!: numeric tower.
//!
//! Exact vs machine representations are distinguished explicitly; promotion
//! and conversion costs are deterministic. A promotion that cannot be
//! represented is a typed refusal, never a silent widening.

use std::fmt::Write as _;

use emath_core::fnv1a64_bytes;

/// Numeric representation family.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum NumKind {
    /// Exact integer.
    Integer,
    /// Exact reduced rational (i128 numerator/denominator).
    Rational,
    /// Binary floating point.
    Float,
    /// Decimal with a fixed exponent scale.
    Decimal,
}

/// A numeric type on the deterministic tower.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct NumericType {
    /// Representation family.
    pub kind: NumKind,
    /// Bit width for fixed-width families (0 = exact/unbounded).
    pub bits: u16,
    /// Whether the type is signed (integers).
    pub signed: bool,
    /// Decimal scale exponent (mantissa * 10^scale).
    pub scale: i16,
}

impl NumericType {
    /// Exact rational type.
    #[must_use]
    pub const fn rational() -> Self {
        Self {
            kind: NumKind::Rational,
            bits: 0,
            signed: true,
            scale: 0,
        }
    }

    /// Exact integer type.
    #[must_use]
    pub const fn integer(signed: bool, bits: u16) -> Self {
        Self {
            kind: NumKind::Integer,
            bits,
            signed,
            scale: 0,
        }
    }

    /// Binary float type.
    #[must_use]
    pub const fn float(bits: u16) -> Self {
        Self {
            kind: NumKind::Float,
            bits,
            signed: true,
            scale: 0,
        }
    }

    /// Decimal type with fixed scale.
    #[must_use]
    pub const fn decimal(scale: i16) -> Self {
        Self {
            kind: NumKind::Decimal,
            bits: 128,
            signed: true,
            scale,
        }
    }

    /// Canonical name, e.g. `i64`, `f64`, `rational`, `decimal(2)`.
    #[must_use]
    pub fn name(&self) -> String {
        match self.kind {
            NumKind::Integer => {
                format!(
                    "{}{bits}",
                    if self.signed { 'i' } else { 'u' },
                    bits = self.bits
                )
            }
            NumKind::Rational => "rational".to_string(),
            NumKind::Float => format!("f{}", self.bits),
            NumKind::Decimal => format!("decimal({})", self.scale),
        }
    }

    /// Deterministic canonical rendering.
    #[must_use]
    pub fn canonical(&self) -> String {
        format!(
            "{:?}:{}:{}:{}",
            self.kind, self.bits, self.signed, self.scale
        )
    }

    /// FNV-1a64 identity.
    #[must_use]
    pub fn identity(&self) -> u64 {
        fnv1a64_bytes(self.canonical().as_bytes())
    }
}

impl Default for NumericType {
    fn default() -> Self {
        Self::float(64)
    }
}

/// Promotion failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NumericError {
    /// Stable code (`E-TYPE-310`/`E-TYPE-311`).
    pub code: &'static str,
    /// Message.
    pub message: String,
}

/// Promotes two numeric types to the smallest type that can represent both,
/// deterministically.
pub fn promote(left: NumericType, right: NumericType) -> Result<NumericType, NumericError> {
    match (left.kind, right.kind) {
        (NumKind::Decimal, NumKind::Decimal) => {
            Ok(NumericType::decimal(left.scale.max(right.scale)))
        }
        (NumKind::Float, NumKind::Float) => Ok(NumericType::float(left.bits.max(right.bits))),
        (NumKind::Integer, NumKind::Integer) => promote_integers(left, right),
        (NumKind::Rational, _) | (_, NumKind::Rational) => Ok(NumericType::rational()),
        (NumKind::Decimal, NumKind::Integer) | (NumKind::Integer, NumKind::Decimal) => {
            let scale = if left.kind == NumKind::Decimal {
                left.scale
            } else {
                right.scale
            };
            Ok(NumericType::decimal(scale))
        }
        (NumKind::Decimal | NumKind::Integer, NumKind::Float)
        | (NumKind::Float, NumKind::Decimal | NumKind::Integer) => Ok(NumericType::float(64)),
    }
}

fn promote_integers(left: NumericType, right: NumericType) -> Result<NumericType, NumericError> {
    let signed = left.signed || right.signed;
    let bits = left.bits.max(right.bits);
    if left.signed != right.signed && bits >= 64 {
        return Err(NumericError {
            code: "E-TYPE-311",
            message: format!(
                "cannot promote {} and {} without exact-width loss",
                left.name(),
                right.name()
            ),
        });
    }
    Ok(NumericType::integer(signed, bits))
}

/// Conversion cost between numeric types (0 = identity, ascending = more
/// lossy/expensive). `None` when the conversion is refused.
#[must_use]
pub fn cast_cost(from: NumericType, to: NumericType) -> Option<u8> {
    if from == to {
        return Some(0);
    }
    match (from.kind, to.kind) {
        (NumKind::Integer, NumKind::Integer) => {
            if to.signed && !from.signed && to.bits <= from.bits {
                return Some(2);
            }
            Some(1)
        }
        (NumKind::Integer | NumKind::Rational, NumKind::Rational | NumKind::Decimal) => Some(2),
        (NumKind::Decimal, NumKind::Decimal) => Some(1),
        (NumKind::Float, NumKind::Float) => Some(if to.bits >= from.bits { 1 } else { 3 }),
        (NumKind::Decimal, NumKind::Integer | NumKind::Rational)
        | (NumKind::Float, NumKind::Decimal | NumKind::Integer | NumKind::Rational) => Some(4),
        (NumKind::Decimal | NumKind::Integer | NumKind::Rational, NumKind::Float) => Some(3),
        (NumKind::Rational, NumKind::Integer) => Some(5),
    }
}

/// Renders a canonical promotion table row for export.
#[must_use]
pub fn tower_rows() -> String {
    let mut out = String::new();
    for kind in [
        NumericType::integer(true, 8),
        NumericType::integer(true, 16),
        NumericType::integer(true, 32),
        NumericType::integer(true, 64),
        NumericType::integer(false, 8),
        NumericType::integer(false, 16),
        NumericType::integer(false, 32),
        NumericType::integer(false, 64),
        NumericType::rational(),
        NumericType::float(32),
        NumericType::float(64),
        NumericType::decimal(2),
    ] {
        let _ = write!(out, "{};", kind.name());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_promotion_is_smallest_sufficient() {
        assert_eq!(
            promote(
                NumericType::integer(true, 8),
                NumericType::integer(true, 32)
            ),
            Ok(NumericType::integer(true, 32))
        );
        assert_eq!(
            promote(
                NumericType::integer(false, 8),
                NumericType::integer(true, 32)
            ),
            Ok(NumericType::integer(true, 32))
        );
    }

    #[test]
    fn mixed_signed_width_64_is_refused() {
        let error = promote(
            NumericType::integer(true, 64),
            NumericType::integer(false, 64),
        )
        .unwrap_err();
        assert_eq!(error.code, "E-TYPE-311");
    }

    #[test]
    fn float_and_decimal_promote_deterministically() {
        assert_eq!(
            promote(NumericType::float(32), NumericType::float(64)),
            Ok(NumericType::float(64))
        );
        assert_eq!(
            promote(NumericType::decimal(2), NumericType::decimal(4)),
            Ok(NumericType::decimal(4))
        );
        assert_eq!(
            promote(NumericType::decimal(2), NumericType::integer(true, 32)),
            Ok(NumericType::decimal(2))
        );
    }

    #[test]
    fn cast_costs_monotone_and_deterministic() {
        assert_eq!(
            cast_cost(NumericType::float(32), NumericType::float(64)),
            Some(1)
        );
        assert_eq!(
            cast_cost(NumericType::float(64), NumericType::float(32)),
            Some(3)
        );
        assert_eq!(
            cast_cost(NumericType::integer(true, 8), NumericType::rational()),
            Some(2)
        );
        assert_eq!(
            cast_cost(NumericType::rational(), NumericType::integer(true, 64)),
            Some(5)
        );
        assert_eq!(
            cast_cost(NumericType::float(64), NumericType::float(64)),
            Some(0)
        );
    }

    #[test]
    fn tower_names_and_identity_are_stable() {
        assert_eq!(NumericType::integer(true, 64).name(), "i64");
        assert_eq!(NumericType::float(64).name(), "f64");
        assert_eq!(NumericType::rational().name(), "rational");
        let first = NumericType::decimal(2);
        assert_eq!(first.identity(), first.identity());
        assert_ne!(first.identity(), NumericType::decimal(3).identity());
        assert!(tower_rows().contains("i8;"));
        assert!(tower_rows().contains("decimal(2);"));
    }
}
