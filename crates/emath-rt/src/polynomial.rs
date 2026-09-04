//! Typed wrappers over the polynomial kernels.
//!
//! The single source of truth lives in [`crate::body`] (deterministic,
//! std-only, embedded verbatim into generated crates). This module adds
//! the typed refusal layer the reference interpreter surfaces:
//! `E-POLY-001` (non-finite coefficient) and `E-POLY-002` (non-finite
//! evaluation point). The EMPTY coefficient vector is the zero
//! polynomial (additive identity — documented algebra, never a shape
//! error). Determinism class: ascending-index convolution, one-pass
//! Horner; identical inputs are bit-identical.

/// Polynomial refusal. Closed set; codes are the language surface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PolyError {
    /// `E-POLY-001` — a coefficient is non-finite.
    NonFiniteCoefficient,
    /// `E-POLY-002` — the evaluation point is non-finite.
    NonFinitePoint,
}

impl PolyError {
    /// Stable diagnostic code (the language surface).
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::NonFiniteCoefficient => "E-POLY-001",
            Self::NonFinitePoint => "E-POLY-002",
        }
    }
}

impl std::fmt::Display for PolyError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonFiniteCoefficient => write!(
                formatter,
                "{code}: polynomial coefficients must be finite",
                code = self.code()
            ),
            Self::NonFinitePoint => write!(
                formatter,
                "{code}: the evaluation point must be finite",
                code = self.code()
            ),
        }
    }
}

impl std::error::Error for PolyError {}

/// Cauchy convolution of two coefficient vectors (ascending order).
///
/// # Errors
/// A non-finite coefficient in either operand is typed
/// (`E-POLY-001`).
pub fn poly_mul(a: &[f64], b: &[f64]) -> Result<Vec<f64>, PolyError> {
    if a.iter()
        .chain(b.iter())
        .any(|coefficient| !coefficient.is_finite())
    {
        return Err(PolyError::NonFiniteCoefficient);
    }
    Ok(crate::body::poly_mul(a, b))
}

/// Horner evaluation of a coefficient vector (ascending order) at
/// `point`; empty coefficients evaluate to 0.0 (the zero polynomial).
///
/// # Errors
/// A non-finite coefficient or point is typed (`E-POLY-001` /
/// `E-POLY-002`).
pub fn poly_eval(coefficients: &[f64], point: f64) -> Result<f64, PolyError> {
    if coefficients
        .iter()
        .chain(std::iter::once(&point))
        .any(|value| !value.is_finite())
    {
        return Err(if point.is_finite() {
            PolyError::NonFiniteCoefficient
        } else {
            PolyError::NonFinitePoint
        });
    }
    Ok(crate::body::poly_eval(coefficients, point))
}
