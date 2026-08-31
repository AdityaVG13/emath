//! Typed spectral-PDE solve (xx0x.4 thin nucleus slice): the error
//! model and safe wrapper over the strict-f64 kernel in `crate::body`.
//!
//! Capability bounds, honestly named: 1D, unit interval, uniform grid,
//! Dirichlet class only (`u(0) = u(1) = 0`). Non-Dirichlet BC classes,
//! FEM assembly, and multi-dimensional spectral solves are named
//! deferrals of the bead, not claims of this slice.

use crate::body::poisson_dirichlet_sine as kernel;

/// Typed refusal for the spectral Poisson solve (xx0x.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PdeError {
    /// An empty interior has no nodes and no solve (E-PDE-001).
    EmptyInterior,
    /// A non-finite load sample must never silently corrupt the field
    /// (E-PDE-002).
    NonFinite,
}

impl PdeError {
    /// Stable diagnostic code (the seed-visible shape).
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::EmptyInterior => "E-PDE-001",
            Self::NonFinite => "E-PDE-002",
        }
    }
}

/// Solve `-u'' = f` on [0,1] with `u(0) = u(1) = 0` by the discrete
/// sine diagonalization. `load` holds the `n` interior load samples
/// (`h = 1/(n+1)`); the returned field holds the `n` interior solution
/// samples (boundary values are 0 by the BC class and are not carried).
pub fn poisson_dirichlet_sine(load: &[f64]) -> Result<Vec<f64>, PdeError> {
    if load.is_empty() {
        return Err(PdeError::EmptyInterior);
    }
    if load.iter().any(|value| !value.is_finite()) {
        return Err(PdeError::NonFinite);
    }
    let field = kernel(load);
    if field.len() != load.len() || field.iter().any(|value| !value.is_finite()) {
        // Unreachable for finite loads (the diagonalization is exact
        // and the eigenvalues are strictly positive); fail closed
        // rather than return a wrong field.
        return Err(PdeError::NonFinite);
    }
    Ok(field)
}
