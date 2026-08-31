//! Typed probability surface (xx0x.5 thin nucleus slice): the error
//! model and safe wrappers over the strict-f64 kernels in
//! `crate::body`.
//!
//! ONE generator story lives here: the explicit source seed enters the
//! `emath-gap-stochastic-vnqo` counter-based root-stream contract, whose
//! counter-zero value deterministically seeds the local sampling kernel.
//!
//! Capability bounds, honestly named: three admitted families
//! (Normal, Uniform, Bernoulli) with exact densities. MCMC/Bayesian
//! posterior sampling, UQ, and random-matrix theory are the bead's
//! named deferrals, not claims of this slice.

use crate::body::{prob_density as kernel_density, prob_sample as kernel_sample};
use emath_core::stochastic::{Seed, StreamPath, local_stream_seed};

/// The admitted distribution families (the op payload encodes these
/// as `u8`: 0 = Normal, 1 = Uniform, 2 = Bernoulli).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    /// Normal(μ, σ), Box–Muller sampling.
    Normal,
    /// Uniform(a, b), affine map of [0, 1).
    Uniform,
    /// Bernoulli(p), threshold sampling; p ∈ {0, 1} exact.
    Bernoulli,
}

impl Family {
    /// The kernel's `u8` encoding (stable; codegen renders it).
    #[must_use]
    pub fn code(self) -> u8 {
        match self {
            Self::Normal => 0,
            Self::Uniform => 1,
            Self::Bernoulli => 2,
        }
    }

    /// Required parameter arity (ascending carrier): Normal/Uniform
    /// take two, Bernoulli one.
    #[must_use]
    pub fn arity(self) -> usize {
        match self {
            Self::Normal | Self::Uniform => 2,
            Self::Bernoulli => 1,
        }
    }
}

/// Typed refusal for the probability ops (xx0x.5).
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

fn validate(family: Family, params: &[f64]) -> Result<(), ProbError> {
    if params.len() != family.arity() {
        return Err(ProbError::ParamArity);
    }
    if params.iter().any(|value| !value.is_finite()) {
        return Err(ProbError::NonFinite);
    }
    match family {
        Family::Normal if params[1] <= 0.0 => Err(ProbError::InvalidParameter),
        Family::Uniform if params[0] > params[1] => Err(ProbError::InvalidParameter),
        Family::Bernoulli if !(0.0..=1.0).contains(&params[0]) => Err(ProbError::InvalidParameter),
        _ => Ok(()),
    }
}

/// Sample `draws` values from `family` with the given seed. Same seed
/// ⟹ bit-identical draws (the bead's reproducibility law). Zero draws
/// is the legal empty stream; a draw count that is not a non-negative
/// integer or exceeds the compute budget refuses.
pub fn prob_sample(
    family: Family,
    params: &[f64],
    seed: f64,
    draws: f64,
) -> Result<Vec<f64>, ProbError> {
    prob_sample_in_stream(family, params, seed, draws, "")
}

/// Sample from one declared stream path. Dot-separated labels define split
/// topology; the empty spelling is the root stream.
pub fn prob_sample_in_stream(
    family: Family,
    params: &[f64],
    seed: f64,
    draws: f64,
    stream_path: &str,
) -> Result<Vec<f64>, ProbError> {
    validate(family, params)?;
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
        family.code(),
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

/// The density (PMF for Bernoulli) of `family` at `x` — exact closed
/// forms, not estimates.
pub fn prob_density(family: Family, params: &[f64], x: f64) -> Result<f64, ProbError> {
    validate(family, params)?;
    if !x.is_finite() {
        return Err(ProbError::NonFinite);
    }
    kernel_density(family.code(), params, x).ok_or(ProbError::NonFinite)
}
