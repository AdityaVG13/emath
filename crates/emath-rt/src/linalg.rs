//! Typed wrappers over the richer linear-algebra kernels (xx0x.2).
//!
//! The single source of truth lives in [`crate::body`] (flat row-major,
//! std-only, embedded verbatim into generated crates). This module adds
//! the typed refusal layer the interpreter surfaces: `E-LINALG-001/2`
//! (spectral input classes) and `E-LINALG-003` (iterative
//! non-convergence) — empty kernel output means refusal, never NaN.
//! Determinism class: fixed sweep order, closed-form rotations, stable
//! sorts; identical inputs are bit-identical.

/// Linear-algebra refusal. Closed set; codes are the language surface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinalgError {
    /// `E-LINALG-001` — the spectral operation requires a square matrix.
    NonSquare {
        /// Actual row count.
        rows: usize,
        /// Actual column count.
        cols: usize,
    },
    /// `E-LINALG-002` — the matrix is materially non-symmetric.
    NotSymmetric,
    /// `E-LINALG-003` — the iterative solve did not converge (non-SPD
    /// or indefinite system within the iteration budget).
    NotConverged {
        /// Iterations spent before the refusal.
        iterations: usize,
    },
    /// `E-LINALG-004` — operand dimensions do not compose.
    ShapeMismatch {
        /// Short reason.
        detail: &'static str,
    },
}

impl LinalgError {
    /// Stable diagnostic code (the language surface).
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::NonSquare { .. } => "E-LINALG-001",
            Self::NotSymmetric => "E-LINALG-002",
            Self::NotConverged { .. } => "E-LINALG-003",
            Self::ShapeMismatch { .. } => "E-LINALG-004",
        }
    }
}

impl std::fmt::Display for LinalgError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonSquare { rows, cols } => write!(
                formatter,
                "{code}: eigen requires a square matrix, got {rows}x{cols}",
                code = self.code()
            ),
            Self::NotSymmetric => write!(
                formatter,
                "{code}: eigen requires a symmetric matrix (A = Aᵀ)",
                code = self.code()
            ),
            Self::NotConverged { iterations } => write!(
                formatter,
                "{code}: iterative solve did not converge within {iterations} iterations \
                 (the system may be non-SPD or indefinite)",
                code = self.code()
            ),
            Self::ShapeMismatch { detail } => {
                write!(formatter, "{code}: {detail}", code = self.code())
            }
        }
    }
}

impl std::error::Error for LinalgError {}

/// Eigenvalue decomposition of a real symmetric square matrix (flat
/// row-major): `(values ascending, vectors columns-aligned)` via the
/// [`crate::body::eig_symmetric`] kernel. Non-square input is refused
/// before the kernel runs (`E-LINALG-001`); the kernel's own empty
/// output maps to the symmetric-class refusal (`E-LINALG-002`) or a
/// convergence stall.
///
/// # Errors
/// Non-square, non-symmetric, or stalled input is typed.
pub fn jacobi_eigen(
    flat: &[f64],
    rows: usize,
    cols: usize,
) -> Result<(Vec<f64>, Vec<Vec<f64>>), LinalgError> {
    if rows != cols {
        return Err(LinalgError::NonSquare { rows, cols });
    }
    let (values, vectors) = crate::body::eig_symmetric(flat, rows, cols);
    if values.is_empty() {
        // The kernel gates both non-symmetric input and convergence
        // stalls; without re-deriving which, the honest typed mapping
        // is the symmetric-class refusal (the common cause).
        return Err(LinalgError::NotSymmetric);
    }
    Ok((values, vectors))
}

/// Singular values (DESCENDING) of a rectangular matrix (flat
/// row-major) via the [`crate::body::svd_values_flat`] kernel.
///
/// # Errors
/// Empty or non-finite input is typed (`E-LINALG-004`).
pub fn svd_singular_values(
    flat: &[f64],
    rows: usize,
    cols: usize,
) -> Result<Vec<f64>, LinalgError> {
    if rows == 0 || cols == 0 || flat.len() != rows * cols {
        return Err(LinalgError::ShapeMismatch {
            detail: "svd requires a non-empty rectangular matrix",
        });
    }
    if flat.iter().any(|x| !x.is_finite()) {
        return Err(LinalgError::ShapeMismatch {
            detail: "svd requires finite entries",
        });
    }
    let values = crate::body::svd_values_flat(flat, rows, cols);
    if values.is_empty() {
        return Err(LinalgError::ShapeMismatch {
            detail: "svd requires a non-empty finite rectangular matrix",
        });
    }
    Ok(values)
}

/// Packed `[U; s; Vᵀ]` thin-SVD factors (see the EMIR op docs) via the
/// [`crate::body::svd_factors_flat`] kernel.
///
/// # Errors
/// Empty or non-finite input is typed (`E-LINALG-004`).
pub fn svd_factors_packed(flat: &[f64], rows: usize, cols: usize) -> Result<Vec<f64>, LinalgError> {
    if rows == 0 || cols == 0 || flat.len() != rows * cols {
        return Err(LinalgError::ShapeMismatch {
            detail: "svd requires a non-empty rectangular matrix",
        });
    }
    let packed = crate::body::svd_factors_flat(flat, rows, cols);
    if packed.is_empty() {
        return Err(LinalgError::ShapeMismatch {
            detail: "svd requires a non-empty finite rectangular matrix",
        });
    }
    Ok(packed)
}

/// Conjugate gradient over flat row-major dense storage: solves
/// `A x = b` for SPD `A` via the [`crate::body::cg_solve_flat`] kernel.
///
/// # Errors
/// Dimension mismatch (`E-LINALG-004`) and non-convergence
/// (`E-LINALG-003`, the system may be non-SPD or indefinite) are typed.
pub fn cg_solve(
    a_flat: &[f64],
    rows: usize,
    cols: usize,
    b: &[f64],
) -> Result<Vec<f64>, LinalgError> {
    if rows != cols {
        return Err(LinalgError::NonSquare { rows, cols });
    }
    if b.len() != rows {
        return Err(LinalgError::ShapeMismatch {
            detail: "right-hand side length must match the matrix order",
        });
    }
    let x = crate::body::cg_solve_flat(a_flat, rows, cols, b);
    if x.is_empty() {
        return Err(LinalgError::NotConverged { iterations: 200 });
    }
    Ok(x)
}

/// Dense partial-pivot solve for any nonsingular square system.
pub fn linear_solve(
    a_flat: &[f64],
    rows: usize,
    cols: usize,
    b: &[f64],
) -> Result<Vec<f64>, LinalgError> {
    if rows != cols {
        return Err(LinalgError::NonSquare { rows, cols });
    }
    if b.len() != rows {
        return Err(LinalgError::ShapeMismatch {
            detail: "right-hand side length must match the matrix order",
        });
    }
    let solution = crate::body::linear_solve_flat(a_flat, rows, cols, b);
    if solution.is_empty() {
        return Err(LinalgError::ShapeMismatch {
            detail: "linear system must be finite and nonsingular",
        });
    }
    Ok(solution)
}

/// Packed `[p; L; U]` partial-pivot LU factors.
pub fn lu_factors(a_flat: &[f64], rows: usize, cols: usize) -> Result<Vec<f64>, LinalgError> {
    if rows != cols {
        return Err(LinalgError::NonSquare { rows, cols });
    }
    let factors = crate::body::lu_factors_flat(a_flat, rows, cols);
    if factors.is_empty() {
        return Err(LinalgError::ShapeMismatch {
            detail: "LU factorization requires a finite nonsingular matrix",
        });
    }
    Ok(factors)
}

/// Packed `[Q; R]` thin QR factors.
pub fn qr_factors(a_flat: &[f64], rows: usize, cols: usize) -> Result<Vec<f64>, LinalgError> {
    let factors = crate::body::qr_factors_flat(a_flat, rows, cols);
    if factors.is_empty() {
        return Err(LinalgError::ShapeMismatch {
            detail: "thin QR requires a finite full-rank matrix with rows >= columns",
        });
    }
    Ok(factors)
}
