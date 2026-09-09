//! Linear-algebra refusal codes shared with authored capsule reference bodies.
//!
//! Norms, products, decompositions, and solvers execute from executable
//! language/spec methods; this module retains only the closed diagnostic
//! set that is the language surface (`E-LINALG-001/2/3/4`).

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
