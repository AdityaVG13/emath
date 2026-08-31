//! Typed wrappers over the optimization kernels (r3-lp-milp-wlif
//! slice 1).
//!
//! The single source of truth lives in [`crate::body`] (deterministic,
//! std-only, embedded verbatim into generated crates). This module adds
//! the typed refusal layer the reference interpreter surfaces:
//! `E-LP-001` (unbounded objective), `E-LP-002` (right-hand side
//! outside the standard-form class `b ≥ 0` — normalization is a named
//! deferral), `E-LP-003` (operand dimensions do not compose), `E-LP-004`
//! (non-finite entries), `E-PARETO-001` (non-finite objective entry),
//! `E-PARETO-002` (empty objective carrier). Empty kernel output means
//! refusal — never a wrong "optimum". Determinism class: Bland's-rule
//! simplex (smallest-index rules throughout, provably terminating);
//! identical inputs are bit-identical.

/// Linear-programming refusal. Closed set; codes are the language
/// surface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LpError {
    /// `E-LP-001` — the objective is unbounded on the feasible set.
    Unbounded,
    /// `E-LP-002` — the right-hand side has a negative entry: outside
    /// the standard-form class (`b ≥ 0`); normalization is a named
    /// deferral, not a silent transform.
    NonStandardForm,
    /// `E-LP-003` — operand dimensions do not compose.
    ShapeMismatch {
        /// Short reason.
        detail: &'static str,
    },
    /// `E-LP-004` — a coefficient is non-finite.
    NonFinite,
    /// `E-LP-005` — the simplex iteration cap was hit (unreachable
    /// under Bland's rule; a bug guard, never a policy).
    IterationBudget,
}

impl LpError {
    /// Stable diagnostic code (the language surface).
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Unbounded => "E-LP-001",
            Self::NonStandardForm => "E-LP-002",
            Self::ShapeMismatch { .. } => "E-LP-003",
            Self::NonFinite => "E-LP-004",
            Self::IterationBudget => "E-LP-005",
        }
    }
}

impl std::fmt::Display for LpError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unbounded => write!(
                formatter,
                "{code}: the linear objective is unbounded on the feasible set",
                code = self.code()
            ),
            Self::NonStandardForm => write!(
                formatter,
                "{code}: standard-form linear programs require b ≥ 0 \
                 (normalize negative right sides before solving)",
                code = self.code()
            ),
            Self::ShapeMismatch { detail } => {
                write!(formatter, "{code}: {detail}", code = self.code())
            }
            Self::NonFinite => write!(
                formatter,
                "{code}: linear-program coefficients must be finite",
                code = self.code()
            ),
            Self::IterationBudget => write!(
                formatter,
                "{code}: simplex iteration budget exceeded",
                code = self.code()
            ),
        }
    }
}

impl std::error::Error for LpError {}

/// Pareto-front refusal. Closed set; codes are the language surface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParetoError {
    /// `E-PARETO-001` — an objective entry is non-finite.
    NonFinite,
    /// `E-PARETO-002` — the objective carrier is empty or ragged.
    ShapeMismatch,
}

impl ParetoError {
    /// Stable diagnostic code (the language surface).
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::NonFinite => "E-PARETO-001",
            Self::ShapeMismatch => "E-PARETO-002",
        }
    }
}

impl std::fmt::Display for ParetoError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonFinite => write!(
                formatter,
                "{code}: objective entries must be finite",
                code = self.code()
            ),
            Self::ShapeMismatch => write!(
                formatter,
                "{code}: the objective carrier must be a non-empty rectangular matrix",
                code = self.code()
            ),
        }
    }
}

impl std::error::Error for ParetoError {}

/// Minimize `cᵀx` s.t. `A x ≤ b`, `x ≥ 0` (standard form, `b ≥ 0`) via
/// the [`crate::body::lp_minimize`] kernel. `A` is flat row-major
/// `m×n`; `b` has length `m`; `c` has length `n`.
///
/// # Errors
/// Dimension mismatch (`E-LP-003`), non-finite coefficients
/// (`E-LP-004`), a negative right side (`E-LP-002`), and an unbounded
/// objective (`E-LP-001`) are typed.
pub fn lp_minimize(
    a_flat: &[f64],
    m: usize,
    n: usize,
    b: &[f64],
    c: &[f64],
) -> Result<Vec<f64>, LpError> {
    if m == 0 || n == 0 || a_flat.len() != m * n {
        return Err(LpError::ShapeMismatch {
            detail: "the constraint matrix must be a non-empty m×n matrix",
        });
    }
    if b.len() != m {
        return Err(LpError::ShapeMismatch {
            detail: "the right-hand side length must equal the constraint count",
        });
    }
    if c.len() != n {
        return Err(LpError::ShapeMismatch {
            detail: "the objective length must equal the variable count",
        });
    }
    if a_flat.iter().any(|v| !v.is_finite())
        || b.iter().any(|v| !v.is_finite())
        || c.iter().any(|v| !v.is_finite())
    {
        return Err(LpError::NonFinite);
    }
    if b.iter().any(|v| *v < 0.0) {
        return Err(LpError::NonStandardForm);
    }
    let a_nested: Vec<Vec<f64>> = (0..m)
        .map(|row| a_flat[row * n..row * n + n].to_vec())
        .collect();
    let x = crate::body::lp_minimize(&a_nested, b, c);
    if x.is_empty() {
        // The wrapper pre-refused every other empty cause; the kernel's
        // remaining empty output is the unbounded objective (the cap is
        // a bug guard, unreachable under Bland's rule).
        return Err(LpError::Unbounded);
    }
    Ok(x)
}

/// Non-dominated mask over the finite objective carrier (all
/// MINIMIZED; rows are points, in mask order — the portfolio
/// artifact's deterministic data).
///
/// # Errors
/// Non-finite entries (`E-PARETO-001`) and an empty carrier
/// (`E-PARETO-002`) are typed.
pub fn pareto_front(
    points_flat: &[f64],
    rows: usize,
    cols: usize,
) -> Result<Vec<f64>, ParetoError> {
    if rows == 0 || cols == 0 || points_flat.len() != rows * cols {
        return Err(ParetoError::ShapeMismatch);
    }
    if points_flat.iter().any(|v| !v.is_finite()) {
        return Err(ParetoError::NonFinite);
    }
    let points: Vec<Vec<f64>> = (0..rows)
        .map(|row| points_flat[row * cols..row * cols + cols].to_vec())
        .collect();
    let mask = crate::body::pareto_front(&points);
    if mask.is_empty() {
        return Err(ParetoError::ShapeMismatch);
    }
    Ok(mask)
}
