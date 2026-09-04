//! Scalar SDE kernel: deterministic,
//! generic, zero new randomness machinery.
//!
//! Contract (mirrors the ODE empty-law convention):
//! - **Carrier**: dX = μ(X) dt + σ(X) dW where μ and σ are ASCENDING
//!   polynomial coefficient carriers (the B28 law):
//!   μ(x) = Σ aᵢxⁱ, σ(x) = Σ bᵢxⁱ. The empty carrier is the zero
//!   polynomial; explicit zeros are allowed (σ ≡ 0 ⟹ the ODE
//!   reduction).
//! - **Rules** (mathematically distinct, both executable):
//!   - Itô (Euler–Maruyama): X' = X + μ(X)·h + σ(X)·√h·Z.
//!   - Stratonovich (corrected midpoint, Euler–Heun form):
//!     X' = X + μ(X)·h + σ(X)·√h·Z + ½·σ(X)·σ'(X)·h·Z².
//!   σ' is the exact derivative of the ascending carrier; for additive
//!   noise (σ' = 0) the rules agree bit-for-bit — never approximate.
//! - **Noise**: one standard Normal Z per step, drawn from the SAME
//!   deterministic path the `ProbSample` Normal(0,1) cell uses: the
//!   explicit seed word enters `local_stream_seed(Seed, root)` (the
//! counter-based Philox contract), that state drives the
//!   SplitMix64 stepper, and each Z is one Box–Muller pair of
//!   uniforms. No ambient entropy; no hidden seed; same seed ⟹
//!   bit-identical trajectory. The seed is a required parameter
//!   (Option only at the Rust seam so "absent" is representable; the
//!   language surface refuses omission with `E-SIM-SEED`).
//! - **Refusals (typed, never silent)**: a missing, non-finite,
//!   negative, or ≥ 2⁶⁴ seed refuses `E-SIM-SEED`; a non-finite
//!   drift/diffusion/state/step refuses `E-SIM-001`; a domain error
//!   (h ≤ 0, zero steps) refuses `E-SIM-002`; an over-budget step
//!   count refuses `E-SIM-003` (no unbounded allocation).
//!
//! No-claim boundaries: only scalar SDEs with polynomial drift/
//! diffusion; multi-dimensional systems, general non-polynomial
//! coefficients, adaptive stepping, and strong/weak error estimation
//! are named deferrals. `SdeRule` is kernel DATA (like `Family`),
//! never an EmirOp or core kind.

use crate::probability::{Family, prob_sample};

/// The step-count compute budget: beyond this the kernel refuses
/// rather than allocating an unbounded stream (strict-f64 policy, the
/// same bound as the sampler's `MAX_DRAWS`).
pub const SDE_MAX_STEPS: usize = 1 << 20;

/// The integration rule: Itô (Euler–Maruyama) or Stratonovich
/// (corrected midpoint / Euler–Heun). Kernel data — never a core kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SdeRule {
    /// X' = X + μ(X)·h + σ(X)·√h·Z.
    Ito,
    /// X' = X + μ(X)·h + σ(X)·√h·Z + ½·σ(X)·σ'(X)·h·Z².
    Stratonovich,
}

/// Typed refusal for the SDE kernel (the language surface's codes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SdeError {
    /// `E-SIM-SEED` — the seed is missing, non-finite, negative, or
    /// ≥ 2⁶⁴: a deterministic run without a legal explicit seed is
    /// refused, never silently replaced.
    Seed,
    /// `E-SIM-001` — a non-finite drift/diffusion coefficient, state,
    /// or step size.
    NonFinite,
    /// `E-SIM-002` — a domain error: non-positive step size, or zero
    /// steps.
    Domain,
    /// `E-SIM-003` — the step count exceeds the compute budget.
    Budget,
}

impl SdeError {
    /// Stable diagnostic code (the language surface).
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Seed => "E-SIM-SEED",
            Self::NonFinite => "E-SIM-001",
            Self::Domain => "E-SIM-002",
            Self::Budget => "E-SIM-003",
        }
    }
}

impl std::fmt::Display for SdeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Seed => write!(
                formatter,
                "{}: an SDE run needs an explicit finite seed in [0, 2^64); omission is refused, never ambient",
                self.code()
            ),
            Self::NonFinite => write!(
                formatter,
                "{}: SDE carriers, state, and step size must be finite",
                self.code()
            ),
            Self::Domain => write!(
                formatter,
                "{}: a positive finite step size and at least one step are required",
                self.code()
            ),
            Self::Budget => write!(
                formatter,
                "{}: the SDE step count exceeds the compute budget",
                self.code()
            ),
        }
    }
}

impl std::error::Error for SdeError {}

/// B28 ascending-carrier evaluation (the polynomial law shared with
/// the ODE/control kernels); the empty carrier is the zero polynomial.
fn poly_at(coeffs: &[f64], x: f64) -> f64 {
    coeffs.iter().rev().fold(0.0, |acc, &c| acc * x + c)
}

/// Exact derivative of an ascending carrier (σ' for the Stratonovich
/// correction). d/dx Σ cᵢxⁱ = Σ i·cᵢ·xⁱ⁻¹.
fn poly_deriv(coeffs: &[f64]) -> Vec<f64> {
    coeffs
        .iter()
        .enumerate()
        .skip(1)
        .map(|(i, &c)| c * i as f64)
        .collect()
}

/// The legal seed domain is the integer-seed space `[0, 2^64)` as f64
/// values (2^64 is exactly representable). The stream itself is seeded
/// by the value's bit pattern, the same convention `prob_sample` uses.
const SEED_LIMIT: f64 = 18_446_744_073_709_551_616.0;

fn validate_seed(seed: Option<f64>) -> Result<f64, SdeError> {
    let Some(seed) = seed else {
        return Err(SdeError::Seed);
    };
    if !seed.is_finite() || seed < 0.0 || seed >= SEED_LIMIT {
        return Err(SdeError::Seed);
    }
    Ok(seed)
}

/// Euler–Maruyama execution of the scalar SDE under the chosen rule.
///
/// Returns the trajectory `[x0, x1, ..., xN]` of length `steps + 1`.
///
/// # Errors
/// [`SdeError::Seed`] for a missing/invalid seed, [`SdeError::NonFinite`]
/// for non-finite carriers/state/step, [`SdeError::Domain`] for
/// h ≤ 0 or zero steps, [`SdeError::Budget`] for an over-budget step
/// count.
pub fn sde_euler_maruyama(
    rule: SdeRule,
    drift: &[f64],
    diffusion: &[f64],
    x0: f64,
    h: f64,
    steps: usize,
    seed: Option<f64>,
) -> Result<Vec<f64>, SdeError> {
    let seed = validate_seed(seed)?;
    if drift
        .iter()
        .chain(diffusion.iter())
        .chain(std::iter::once(&x0))
        .chain(std::iter::once(&h))
        .any(|value| !value.is_finite())
    {
        return Err(SdeError::NonFinite);
    }
    if !(h > 0.0) || steps == 0 {
        return Err(SdeError::Domain);
    }
    if steps > SDE_MAX_STEPS {
        return Err(SdeError::Budget);
    }
    // The Z stream: the SAME deterministic Normal(0,1) draws the
    // `ProbSample` cell yields for this seed (seed → local stream
    // seed → SplitMix64 → Box–Muller pair per step). Compose, never
    // re-implement: one seed ⟹ one stream, identical to the sampler.
    // Unreachable-by-construction fallback: the seed was validated
    // above (the sampler only rejects non-finite seeds) and
    // SDE_MAX_STEPS mirrors the sampler's MAX_DRAWS (1 << 20), so the
    // draw-count check also passed before this call.
    let zs =
        prob_sample(Family::Normal, &[0.0, 1.0], seed, steps as f64).map_err(|_| SdeError::Seed)?;
    let sqrt_h = h.sqrt();
    // The Stratonovich correction needs σ' once per run; Itô never
    // touches it (no unused allocation on the Itô hot path).
    let d_sigma = match rule {
        SdeRule::Ito => Vec::new(),
        SdeRule::Stratonovich => poly_deriv(diffusion),
    };
    let mut xs = Vec::with_capacity(steps + 1);
    let mut x = x0;
    xs.push(x);
    for &z in &zs {
        // One combined step, applied in the DOCUMENTED sequential
        // order (IEEE-associativity-observable): X + μ(X)·h + noise,
        // then for Stratonovich the ½·σ(Xₙ)·σ'(Xₙ)·h·Z² correction —
        // μ, σ, and σ' all evaluated at the pre-step state Xₙ.
        let mu = poly_at(drift, x);
        let sigma = poly_at(diffusion, x);
        let noise = sigma * sqrt_h * z;
        match rule {
            SdeRule::Ito => x = x + mu * h + noise,
            SdeRule::Stratonovich => {
                let correction = 0.5 * sigma * poly_at(&d_sigma, x) * h * z * z;
                x = x + mu * h + noise + correction;
            }
        }
        xs.push(x);
    }
    Ok(xs)
}
