//! Numeric tower.
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
    if left.signed != right.signed {
        // Mixed-sign promotion must be exact: the result is the wider side
        // when it can represent the whole narrow side; equal widths cannot
        // represent both ranges and are refused at any width.
        let (unsigned_bits, signed_bits) = if left.signed {
            (right.bits, left.bits)
        } else {
            (left.bits, right.bits)
        };
        if unsigned_bits > signed_bits {
            return Ok(NumericType::integer(false, unsigned_bits));
        }
        if signed_bits > unsigned_bits {
            return Ok(NumericType::integer(true, signed_bits));
        }
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

/// Computation model selected by `numeric:` / `representation`.
/// These are computation descriptors, never claims about real-number
/// semantics; `Real` is not silently `f64` (Phase 1 default: `strict-f64`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumericProfile {
    /// IEEE-754 binary64, round-ties-to-even, overflow is error. Phase 1 default.
    StrictF64,
    /// Interval enclosure over binary64 endpoints. Explicit only; never a default.
    IntervalF64,
}

impl NumericProfile {
    /// Stable surface name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::StrictF64 => "strict-f64",
            Self::IntervalF64 => "interval-f64",
        }
    }

    /// Phase 1 default when `numeric:` is omitted.
    #[must_use]
    pub const fn default_phase1() -> Self {
        Self::StrictF64
    }
}

impl Default for NumericProfile {
    fn default() -> Self {
        Self::StrictF64
    }
}

/// Deterministic behavior descriptor for a selected numeric model.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NumericBehavior {
    /// Surface name (`strict-f64`, `interval-f64`).
    pub name: &'static str,
    /// Rounding mode the model commits to.
    pub rounding: &'static str,
    /// Overflow policy the model commits to.
    pub overflow: &'static str,
    /// Determinism class token.
    pub determinism: &'static str,
    /// Maximum significand bits the model can honor.
    pub max_precision_bits: u16,
}

/// Binary64 significand bits (including the implicit 1).
pub const STRICT_F64_PRECISION_BITS: u16 = 53;

/// Binary64 machine epsilon (`2^-52`).
pub const STRICT_F64_MACHINE_EPS: f64 = 2.220_446_049_250_313e-16;

/// Parses a numeric-model name. The empty string is the Phase 1 default
/// (`strict-f64`). Unknown names are typed refusals (`E-NUM-001`).
pub fn parse_numeric_profile(name: &str) -> Result<NumericProfile, NumericError> {
    match name {
        "" | "strict-f64" | "StrictF64" | "Float64" | "float64" | "f64" => {
            Ok(NumericProfile::StrictF64)
        }
        "interval-f64" | "IntervalF64" | "Interval" | "interval" => Ok(NumericProfile::IntervalF64),
        other => Err(NumericError {
            code: "E-NUM-001",
            message: format!("unknown numeric model `{other}` (known: strict-f64, interval-f64)"),
        }),
    }
}

/// Deterministic behavior descriptor for `profile`.
#[must_use]
pub fn numeric_behavior(profile: NumericProfile) -> NumericBehavior {
    match profile {
        NumericProfile::StrictF64 => NumericBehavior {
            name: "strict-f64",
            rounding: "nearest-even",
            overflow: "error",
            determinism: "ieee754-binary64-round-ties-to-even",
            max_precision_bits: STRICT_F64_PRECISION_BITS,
        },
        NumericProfile::IntervalF64 => NumericBehavior {
            name: "interval-f64",
            rounding: "outward",
            overflow: "error",
            determinism: "binary64-endpoint-interval-outward",
            max_precision_bits: STRICT_F64_PRECISION_BITS,
        },
    }
}

/// Refuses a precision demand no selected model can honor (`E-NUM-002`).
pub fn check_precision_demand(profile: NumericProfile, bits: u16) -> Result<(), NumericError> {
    let behavior = numeric_behavior(profile);
    if bits == 0 || bits > behavior.max_precision_bits {
        return Err(NumericError {
            code: "E-NUM-002",
            message: format!(
                "precision demand of {bits} bits cannot be honored by {} (max {} bits)",
                behavior.name, behavior.max_precision_bits
            ),
        });
    }
    Ok(())
}

/// Refuses an error-limit demand no selected model can honor (`E-NUM-003`).
pub fn check_error_limit(profile: NumericProfile, max_abs_error: f64) -> Result<(), NumericError> {
    if !max_abs_error.is_finite() || max_abs_error < 0.0 {
        return Err(NumericError {
            code: "E-NUM-003",
            message: format!("error-limit `{max_abs_error}` is not a finite non-negative bound"),
        });
    }
    match profile {
        NumericProfile::StrictF64 => {
            if max_abs_error == 0.0 || max_abs_error >= STRICT_F64_MACHINE_EPS {
                Ok(())
            } else {
                Err(NumericError {
                    code: "E-NUM-003",
                    message: format!(
                        "strict-f64 cannot honor error-limit {max_abs_error} (tighter than machine epsilon {STRICT_F64_MACHINE_EPS})"
                    ),
                })
            }
        }
        NumericProfile::IntervalF64 => {
            if max_abs_error == 0.0 {
                Err(NumericError {
                    code: "E-NUM-003",
                    message: "interval-f64 cannot honor a zero error-limit (enclosures are not exact reals)".into(),
                })
            } else {
                Ok(())
            }
        }
    }
}
