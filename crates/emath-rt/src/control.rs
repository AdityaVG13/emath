//! Typed control-surface wrappers:
//! the error model over the raw kernels in [`crate::body`].
//!
//! Refusals are typed, never silent: a non-finite carrier
//! (`E-CONTROL-001`), a zero denominator / no value
//! (`E-CONTROL-002`), an unstable state-space carrier
//! (`E-CONTROL-003`), a shape mismatch (`E-CONTROL-004`), and a
//! degenerate Routh table (`E-CONTROL-005`, marginal poles) each name
//! their diagnostic. Representation law: ASCENDING coefficient
//! carriers (the B28 polynomial law); state-space carries implicit
//! D = 0 (the feedthrough term is the named deferral).
//!
//! No-claim boundaries: stability is the Routh–Hurwitz SIGN test over
//! polynomial arithmetic (no eigenvalues, no root-finding, no claimed
//! pole locations); controller design (pole placement, LQR) is not
//! implemented; the Itô/Stratonovich SDE surface (B37) is
//! world-dependent and lives behind the seed/stream contract.
//! Determinism class: fixed-order recurrences, first-index pivot
//! tie-breaking; identical inputs are bit-identical.

/// Control-surface refusal. Closed set; codes are the language surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlError {
    /// `E-CONTROL-001` — a carrier or point is non-finite.
    NonFinite,
    /// `E-CONTROL-002` — the denominator evaluates to zero (a pole
    /// hit, or the zero polynomial): the value does not exist.
    ZeroDenominator,
    /// `E-CONTROL-003` — the state-space carrier is unstable: no DC
    /// gain exists.
    UnstableCarrier,
    /// `E-CONTROL-004` — the state-space carrier is malformed
    /// (non-square A, or b/c length ≠ n).
    ShapeMismatch,
    /// `E-CONTROL-005` — a degenerate Routh table (zero first-column
    /// entry): marginal poles; strict stability is undecidable here.
    MarginalRouth,
}

impl ControlError {
    /// Stable diagnostic code (the language surface).
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::NonFinite => "E-CONTROL-001",
            Self::ZeroDenominator => "E-CONTROL-002",
            Self::UnstableCarrier => "E-CONTROL-003",
            Self::ShapeMismatch => "E-CONTROL-004",
            Self::MarginalRouth => "E-CONTROL-005",
        }
    }
}

impl std::fmt::Display for ControlError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonFinite => write!(
                formatter,
                "{code}: control carriers must be finite",
                code = self.code()
            ),
            Self::ZeroDenominator => write!(
                formatter,
                "{code}: the denominator vanishes — no transfer value exists",
                code = self.code()
            ),
            Self::UnstableCarrier => write!(
                formatter,
                "{code}: the state-space carrier is unstable — no DC gain exists",
                code = self.code()
            ),
            Self::ShapeMismatch => write!(
                formatter,
                "{code}: A must be square with b, c of the state length",
                code = self.code()
            ),
            Self::MarginalRouth => write!(
                formatter,
                "{code}: degenerate Routh table — marginal poles are a named deferral",
                code = self.code()
            ),
        }
    }
}

impl std::error::Error for ControlError {}

/// Transfer-function evaluation `num(x)/den(x)` over ASCENDING
/// carriers.
///
/// # Errors
/// A non-finite carrier/point is typed (`E-CONTROL-001`); a zero
/// denominator (pole hit or the zero polynomial) is typed
/// (`E-CONTROL-002`).
pub fn transfer_eval(num: &[f64], den: &[f64], x: f64) -> Result<f64, ControlError> {
    if num
        .iter()
        .chain(den.iter())
        .chain(std::iter::once(&x))
        .any(|value| !value.is_finite())
    {
        return Err(ControlError::NonFinite);
    }
    if den.is_empty() || den.iter().all(|c| *c == 0.0) || crate::body::poly_eval(den, x) == 0.0 {
        return Err(ControlError::ZeroDenominator);
    }
    Ok(crate::body::control_transfer_eval(num, den, x))
}

/// State-space DC gain `c·(−A)⁻¹·b` (implicit D = 0).
///
/// # Errors
/// A shape mismatch is typed (`E-CONTROL-004`); non-finite entries
/// `E-CONTROL-001`; an unstable carrier `E-CONTROL-003`; a marginal
/// (degenerate Routh) characteristic table `E-CONTROL-005`.
pub fn state_space_dc_gain(a: &[Vec<f64>], b: &[f64], c: &[f64]) -> Result<f64, ControlError> {
    let n = a.len();
    if n == 0 || a.iter().any(|row| row.len() != n) || b.len() != n || c.len() != n {
        return Err(ControlError::ShapeMismatch);
    }
    if a.iter()
        .flatten()
        .chain(b.iter())
        .chain(c.iter())
        .any(|v| !v.is_finite())
    {
        return Err(ControlError::NonFinite);
    }
    match crate::body::control_routh_status(&crate::body::control_char_poly(a)) {
        crate::body::RouthStatus::Stable => {}
        crate::body::RouthStatus::Unstable => return Err(ControlError::UnstableCarrier),
        // A degenerate characteristic table means marginal poles: the
        // DC gain does not exist either.
        crate::body::RouthStatus::Degenerate => return Err(ControlError::MarginalRouth),
        // Unreachable for a monic characteristic polynomial with
        // finite entries (both pre-checked); refused, never invented.
        crate::body::RouthStatus::ZeroPolynomial | crate::body::RouthStatus::NonFinite => {
            return Err(ControlError::MarginalRouth);
        }
    }
    let gain = crate::body::control_state_space_dc_gain(a, b, c);
    // Kernel-bug guard, unreachable for a stable carrier (det ≠ 0):
    // refuse rather than return a non-finite gain.
    if gain.is_finite() {
        Ok(gain)
    } else {
        Err(ControlError::MarginalRouth)
    }
}

/// Routh–Hurwitz strict-stability predicate over an ASCENDING
/// denominator: `Ok(true)` = all roots strictly in the open left half
/// plane; `Ok(false)` = provably unstable.
///
/// # Errors
/// The zero polynomial has no pole set (`E-CONTROL-002`); a degenerate
/// table (zero first-column entry) refuses as marginal
/// (`E-CONTROL-005`); non-finite coefficients `E-CONTROL-001`.
pub fn poles_stable(den: &[f64]) -> Result<bool, ControlError> {
    if den.iter().any(|value| !value.is_finite()) {
        return Err(ControlError::NonFinite);
    }
    match crate::body::control_routh_status(den) {
        crate::body::RouthStatus::Stable => Ok(true),
        crate::body::RouthStatus::Unstable => Ok(false),
        crate::body::RouthStatus::Degenerate => Err(ControlError::MarginalRouth),
        crate::body::RouthStatus::ZeroPolynomial => Err(ControlError::ZeroDenominator),
        crate::body::RouthStatus::NonFinite => Err(ControlError::NonFinite),
    }
}
