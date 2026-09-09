//! Typed probability surface: the error
//! model and safe wrappers over the strict-f64 kernels in
//! `crate::body`.
//!
//! ONE generator story lives here: the explicit source seed enters the
//! counter-based root-stream contract, whose
//! counter-zero value deterministically seeds the local sampling kernel.
//!
//! Capsules own the family names. This wrapper takes the image-supplied
//! kernel code (`0` gaussian, `1` affine support, `2` binary mass) and
//! refuses unknown codes. MCMC/Bayesian posterior sampling, UQ, and
//! random-matrix theory are documented deferrals, not claims of this
//! module.

use crate::body::{prob_density as kernel_density, prob_sample as kernel_sample};
use emath_core::{Seed, StreamPath, local_stream_seed};

/// Required parameter arity for a capsule-supplied kernel code.
#[must_use]
fn kernel_arity(kind: u8) -> Option<usize> {
    match kind {
        0 | 1 => Some(2),
        2 => Some(1),
        _ => None,
    }
}

/// Typed refusal for the probability ops.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProbError {
    /// A parameter outside the family's domain (σ ≤ 0, a > b,
    /// p ∉ [0,1]), or a non-integer / over-budget draw count.
    InvalidParameter,
    /// Non-finite parameter or evaluation point: never a silently
    /// corrupted stream or density.
    NonFinite,
    /// The parameter vector has the wrong length for the family.
    ParamArity,
}

impl ProbError {
    /// Stable diagnostic code (the seed-visible shape).
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::InvalidParameter => "E-PROB-001",
            Self::NonFinite => "E-PROB-002",
            Self::ParamArity => "E-PROB-003",
        }
    }
}

/// Draw-count compute budget: beyond this the sampler refuses rather
/// than silently allocating an unbounded stream (strict-f64 policy:
/// the budget is part of the determinism contract).
const MAX_DRAWS: usize = 1 << 20;

fn validate(kind: u8, params: &[f64]) -> Result<(), ProbError> {
    let Some(arity) = kernel_arity(kind) else {
        return Err(ProbError::InvalidParameter);
    };
    if params.len() != arity {
        return Err(ProbError::ParamArity);
    }
    if params.iter().any(|value| !value.is_finite()) {
        return Err(ProbError::NonFinite);
    }
    match kind {
        0 if params[1] <= 0.0 => Err(ProbError::InvalidParameter),
        1 if params[0] > params[1] => Err(ProbError::InvalidParameter),
        2 if !(0.0..=1.0).contains(&params[0]) => Err(ProbError::InvalidParameter),
        _ => Ok(()),
    }
}

/// Sample `draws` values from the kernel code with the given seed. Same
/// seed ⟹ bit-identical draws (the reproducibility law). Zero draws
/// is the legal empty stream; a draw count that is not a non-negative
/// integer or exceeds the compute budget refuses.
pub fn prob_sample(
    kind: u8,
    params: &[f64],
    seed: f64,
    draws: f64,
) -> Result<Vec<f64>, ProbError> {
    prob_sample_in_stream(kind, params, seed, draws, "")
}

/// Sample from one declared stream path. Dot-separated labels define split
/// topology; the empty spelling is the root stream.
pub fn prob_sample_in_stream(
    kind: u8,
    params: &[f64],
    seed: f64,
    draws: f64,
    stream_path: &str,
) -> Result<Vec<f64>, ProbError> {
    validate(kind, params)?;
    if !seed.is_finite() {
        return Err(ProbError::NonFinite);
    }
    if !draws.is_finite() || draws < 0.0 || draws.fract() != 0.0 || draws as usize > MAX_DRAWS {
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
    let stream = kernel_sample(
        kind,
        params,
        f64::from_bits(local_seed),
        draws as usize,
    );
    if stream.len() != draws as usize {
        // Unreachable after validation; fail closed rather than
        // return a wrong stream.
        return Err(ProbError::NonFinite);
    }
    Ok(stream)
}

/// The density (PMF for the binary-mass kernel) at `x` — exact closed
/// forms, not estimates.
pub fn prob_density(kind: u8, params: &[f64], x: f64) -> Result<f64, ProbError> {
    validate(kind, params)?;
    if !x.is_finite() {
        return Err(ProbError::NonFinite);
    }
    kernel_density(kind, params, x).ok_or(ProbError::NonFinite)
}
