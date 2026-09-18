//! Typed probability surface: the error model and the safe wrapper over
//! the strict-f64 unit-interval kernel in `crate::body`.
//!
//! ONE generator story lives here: the explicit source seed enters the
//! counter-based root-stream contract, whose counter-zero value
//! deterministically seeds the local sampling kernel.
//!
//! The unit-interval stream is the remaining native leaf (bead
//! emath-nwmm6): sampling transforms over it and family densities are
//! authored language definitions; the former sampler/density wrappers
//! were removed with their Rust kernels (orphan cleanup, user-authorized).

use crate::body::prob_unit_interval as kernel_unit_interval;
use emath_core::{Seed, StreamPath, local_stream_seed};

/// Typed refusal for the probability ops.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProbError {
    /// A non-integer or over-budget draw count, or an invalid stream
    /// path (the budget is part of the determinism contract).
    InvalidParameter,
    /// A non-finite seed: never a silently corrupted stream.
    NonFinite,
}

impl ProbError {
    /// Stable diagnostic code (the seed-visible shape).
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::InvalidParameter => "E-PROB-001",
            Self::NonFinite => "E-PROB-002",
        }
    }
}

impl std::fmt::Display for ProbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code())
    }
}

/// Uniform count budget for the unit-interval primitive.
const MAX_UNIFORMS: usize = 2 << 20;

/// Unit-interval uniforms in [0, 1) from an explicit seed and stream path.
/// Same seed and path replay bit-identically. This is the machine
/// counter-stream leaf; distribution transforms are authored above it.
pub fn unit_interval_stream(
    seed: f64,
    draws: f64,
    stream_path: &str,
) -> Result<Vec<f64>, ProbError> {
    if !seed.is_finite() {
        return Err(ProbError::NonFinite);
    }
    if !draws.is_finite() || draws < 0.0 || draws.fract() != 0.0 || draws as usize > MAX_UNIFORMS {
        return Err(ProbError::InvalidParameter);
    }
    let path = if stream_path.is_empty() {
        StreamPath::root()
    } else {
        StreamPath::new(stream_path.split('.').map(str::to_string).collect())
            .map_err(|_| ProbError::InvalidParameter)?
    };
    let local_seed = local_stream_seed(&Seed::new(seed.to_bits()), &path)
        .map_err(|_| ProbError::InvalidParameter)?;
    let n = draws as usize;
    let stream = kernel_unit_interval(f64::from_bits(local_seed), n);
    if stream.len() != n {
        return Err(ProbError::NonFinite);
    }
    Ok(stream)
}
