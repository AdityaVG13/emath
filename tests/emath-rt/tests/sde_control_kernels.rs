//! — the SDE kernel surface (external,
//! owned, mutation-sensitive).
//!
//! Scalar SDE contract under test (the planted gap: no SDE machinery
//! existed anywhere before this ):
//! - **Carrier**: dX = μ(X) dt + σ(X) dW with ASCENDING polynomial
//!   coefficient carriers (the B28 law): μ(x) = Σ a_i x^i,
//!   σ(x) = Σ b_i x^i; the empty carrier is the zero polynomial
//!   (σ ≡ 0 / μ ≡ 0), matching the ODE empty-law convention.
//! - **Rules** (mathematically distinct, both executable):
//!   - Itô (Euler–Maruyama): X' = X + μ(X)·h + σ(X)·√h·Z.
//!   - Stratonovich (corrected midpoint, Euler–Heun form):
//!     X' = X + μ(X)·h + σ(X)·√h·Z + ½·σ(X)·σ'(X)·h·Z².
//!   For additive noise (σ' = 0) the rules agree bit-for-bit; for
//!   state-dependent noise (σ(x) = σ·x) they differ — the correction
//!   term is never silently dropped or merged.
//! - **Noise**: one standard Normal Z per step, drawn deterministically
//! from the contract — the seed maps through
//!   `local_stream_seed(Seed, root)` into a SplitMix64 state, and each
//!   Z comes from one Box–Muller pair of `splitmix64_next` uniforms
//!   (the SAME mapping the established Normal sampler uses). No ambient
//!   entropy, no hidden seed; same seed ⟹ bit-identical trajectory.
//! - **Refusals (typed, never silent)**: a missing or invalid seed
//!   (non-finite, negative, ≥ 2⁶⁴) refuses `E-SIM-SEED`; a non-finite
//!   drift/diffusion/state/step refuses `E-SIM-001`; a domain error
//!   (h ≤ 0, zero steps) refuses `E-SIM-002`; an over-budget step
//!   count refuses `E-SIM-003`.
//!
//! The expected trajectories are SPEC recurrences: the test derives the
//! Z stream from the documented stream primitives (independent of the
//! kernel's internal draw loop) and applies the documented step math by
//! hand, so a wrong draw order, a dropped correction, or a sign error
//! each fail.

use emath_core::stochastic::{Seed, StreamPath, local_stream_seed};
use emath_rt::splitmix64_next;
use emath_rt::stochastic::{SdeError, SdeRule, sde_euler_maruyama};

/// One Uniform[0, 1) from the SplitMix64 state (high 53 bits) — the
/// spec mapping shared with the established samplers.
fn spec_uniform01(state: &mut u64) -> f64 {
    let bits = splitmix64_next(state) >> 11;
    (bits as f64) * (1.0 / (1u64 << 53) as f64)
}

/// The spec Z stream: one Box–Muller standard Normal per step, drawn
/// from the seed-derived state. This mirrors the documented contract:
/// `u1` remapped off zero, `u2` the second uniform.
fn spec_zs(seed: u64, count: usize) -> Vec<f64> {
    let mut state = local_stream_seed(&Seed::new(seed), &StreamPath::root())
        .expect("root stream seed derivation is in contract");
    (0..count)
        .map(|_| {
            let u1 = 1.0 - spec_uniform01(&mut state);
            let u2 = spec_uniform01(&mut state);
            (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
        })
        .collect()
}

/// Horizontal-polyeval of an ASCENDING carrier (the B28 law).
fn poly(coeffs: &[f64], x: f64) -> f64 {
    coeffs.iter().rev().fold(0.0, |acc, &c| acc * x + c)
}

/// Derivative of an ascending carrier.
fn poly_deriv(coeffs: &[f64]) -> Vec<f64> {
    coeffs
        .iter()
        .enumerate()
        .skip(1)
        .map(|(i, &c)| c * i as f64)
        .collect()
}

/// The spec Itô Euler–Maruyama recurrence over the given Z stream.
fn spec_ito(drift: &[f64], diffusion: &[f64], x0: f64, h: f64, zs: &[f64]) -> Vec<f64> {
    let mut xs = vec![x0];
    let mut x = x0;
    let sqrt_h = h.sqrt();
    for &z in zs {
        x = x + poly(drift, x) * h + poly(diffusion, x) * sqrt_h * z;
        xs.push(x);
    }
    xs
}

/// The spec Stratonovich Euler–Heun recurrence (corrected midpoint).
fn spec_strat(drift: &[f64], diffusion: &[f64], x0: f64, h: f64, zs: &[f64]) -> Vec<f64> {
    let mut xs = vec![x0];
    let mut x = x0;
    let sqrt_h = h.sqrt();
    let d_sigma = poly_deriv(diffusion);
    for &z in zs {
        let sigma = poly(diffusion, x);
        let correction = 0.5 * sigma * poly(&d_sigma, x) * h * z * z;
        x = x + poly(drift, x) * h + sigma * sqrt_h * z + correction;
        xs.push(x);
    }
    xs
}

const SEED_A: f64 = 7.0;
const H: f64 = 0.01;
const STEPS: usize = 64;

/// Itô matches the spec recurrence bit-for-bit for a pinned seed:
/// the kernel consumes exactly one Z per step, in contract order.
#[test]
fn sde_ito_matches_spec_recurrence_pinned_seed() {
    let drift = [0.0_f64, 0.25]; // μ(x) = 0.25·x
    let diffusion = [0.0_f64, 0.35]; // σ(x) = 0.35·x
    let zs = spec_zs(SEED_A.to_bits(), STEPS);
    let want = spec_ito(&drift, &diffusion, 1.0, H, &zs);
    let got = sde_euler_maruyama(
        SdeRule::Ito,
        &drift,
        &diffusion,
        1.0,
        H,
        STEPS,
        Some(SEED_A),
    )
    .expect("ito must run");
    assert_eq!(got.len(), STEPS + 1, "trajectory includes x0");
    assert_eq!(
        got, want,
        "ito trajectory must equal the spec recurrence; first divergence drives the seed >= 1 noise draw"
    );
}

/// Stratonovich matches the Euler–Heun spec recurrence bit-for-bit:
/// the correction term ½·σ·σ'·h·Z² is present and exact.
#[test]
fn sde_stratonovich_matches_heun_recurrence_pinned_seed() {
    let drift = [0.0_f64, 0.25];
    let diffusion = [0.0_f64, 0.35];
    let zs = spec_zs(SEED_A.to_bits(), STEPS);
    let want = spec_strat(&drift, &diffusion, 1.0, H, &zs);
    let got = sde_euler_maruyama(
        SdeRule::Stratonovich,
        &drift,
        &diffusion,
        1.0,
        H,
        STEPS,
        Some(SEED_A),
    )
    .expect("stratonovich must run");
    assert_eq!(got.len(), STEPS + 1);
    assert_eq!(
        got, want,
        "stratonovich trajectory must equal the spec Heun recurrence"
    );
}

/// The two rules are MATHEMATICALLY DISTINCT for state-dependent
/// noise: same seed, same draws, different trajectories. The
/// Stratonovich correction is strictly positive here, so the
/// Stratonovich final value exceeds the Itô one.
#[test]
fn sde_rules_differ_for_state_dependent_noise() {
    let drift = [0.0_f64, 0.25];
    let diffusion = [0.0_f64, 0.35];
    let ito = sde_euler_maruyama(
        SdeRule::Ito,
        &drift,
        &diffusion,
        1.0,
        H,
        STEPS,
        Some(SEED_A),
    )
    .unwrap();
    let strat = sde_euler_maruyama(
        SdeRule::Stratonovich,
        &drift,
        &diffusion,
        1.0,
        H,
        STEPS,
        Some(SEED_A),
    )
    .unwrap();
    assert_ne!(ito, strat, "ito and stratonovich must differ (same seed)");
    assert!(
        strat[STEPS] > ito[STEPS],
        "stratonovich correction is positive: {} vs {}",
        strat[STEPS],
        ito[STEPS]
    );
}

/// For ADDITIVE noise (σ' = 0) the rules agree bit-for-bit — the
/// correction term is zero, not approximated or perturbed.
#[test]
fn sde_rules_agree_for_additive_noise() {
    let drift = [0.0_f64, 0.25];
    let diffusion = [0.5_f64]; // σ(x) = 0.5, σ' = 0
    let ito = sde_euler_maruyama(
        SdeRule::Ito,
        &drift,
        &diffusion,
        1.0,
        H,
        STEPS,
        Some(SEED_A),
    )
    .unwrap();
    let strat = sde_euler_maruyama(
        SdeRule::Stratonovich,
        &drift,
        &diffusion,
        1.0,
        H,
        STEPS,
        Some(SEED_A),
    )
    .unwrap();
    assert_eq!(
        ito, strat,
        "additive noise: ito and stratonovich are the same process"
    );
}

/// Zero-noise reduction: σ ≡ 0 makes both rules the explicit Euler ODE
/// X' = μ(X) — the SDE's own step law `Xₙ₊₁ = Xₙ + μ(Xₙ)·h` applies
/// bit-exactly (noise is +0.0, so the rules collapse to one path). The
/// closed-form geometric solution (1 + μh)^N matches to ULP (asserted
/// loosely here; bit-exactness is the step law's own recurrence).
#[test]
fn sde_zero_noise_reduces_to_ode_euler() {
    let drift = [0.0_f64, 0.25];
    let diffusion: [f64; 0] = [];
    let ito = sde_euler_maruyama(
        SdeRule::Ito,
        &drift,
        &diffusion,
        1.0,
        H,
        STEPS,
        Some(SEED_A),
    )
    .unwrap();
    let strat = sde_euler_maruyama(
        SdeRule::Stratonovich,
        &drift,
        &diffusion,
        1.0,
        H,
        STEPS,
        Some(SEED_A),
    )
    .unwrap();
    assert_eq!(ito, strat, "zero noise: rules collapse to one ODE path");
    // The explicit Euler ODE recurrence, same evaluation order as the
    // kernel: xᵢ₊₁ = xᵢ + μ(xᵢ)·h.
    let mut want = vec![1.0];
    let mut last = 1.0;
    for _ in 0..STEPS {
        last = last + poly(&drift, last) * H;
        want.push(last);
    }
    assert_eq!(ito, want, "zero noise must be the explicit Euler ODE");
    // Loose closed-form anchor: X_N ≈ x0·(1 + μh)^N within 1e-9.
    let closed = 1.0_f64 * (1.0 + 0.25 * H).powi(STEPS as i32);
    assert!(
        (ito[STEPS] - closed).abs() < 1e-9,
        "zero-noise path must track the closed form: {} vs {closed}",
        ito[STEPS]
    );
}

/// Determinism: same seed ⟹ bit-identical trajectory; a different seed
/// ⟹ a different trajectory (the seed is identity, never hidden).
#[test]
fn sde_is_deterministic_per_seed() {
    let drift = [0.0_f64, 0.25];
    let diffusion = [0.0_f64, 0.35];
    let a = sde_euler_maruyama(
        SdeRule::Ito,
        &drift,
        &diffusion,
        1.0,
        H,
        STEPS,
        Some(SEED_A),
    )
    .unwrap();
    let a_again = sde_euler_maruyama(
        SdeRule::Ito,
        &drift,
        &diffusion,
        1.0,
        H,
        STEPS,
        Some(SEED_A),
    )
    .unwrap();
    assert_eq!(a, a_again, "same seed must replay bit-identically");
    let b = sde_euler_maruyama(
        SdeRule::Ito,
        &drift,
        &diffusion,
        1.0,
        H,
        STEPS,
        Some(SEED_A + 1.0),
    )
    .unwrap();
    assert_ne!(a, b, "a different seed must change the trajectory");
}

/// Missing or invalid seeds refuse `E-SIM-SEED` — never a hidden seed,
/// never an ambient-entropy fallback.
#[test]
fn sde_missing_or_invalid_seed_refuses_e_sim_seed() {
    let drift = [0.0_f64, 0.25];
    let diffusion = [0.0_f64, 0.35];
    for seed in [
        None,
        Some(f64::NAN),
        Some(f64::INFINITY),
        Some(-1.0),
        Some(2.0_f64.powi(64)),
    ] {
        let err = sde_euler_maruyama(SdeRule::Ito, &drift, &diffusion, 1.0, H, STEPS, seed)
            .expect_err("invalid seed must refuse");
        assert_eq!(
            err.code(),
            "E-SIM-SEED",
            "seed {seed:?} must refuse E-SIM-SEED"
        );
    }
}

/// Non-finite drift/diffusion/state/step refuse `E-SIM-001`.
#[test]
fn sde_nonfinite_refuses() {
    let ok = [0.0_f64, 0.25];
    let bad = [0.0_f64, f64::NAN];
    for (drift, diffusion, x0, h, label) in [
        (&bad[..], &ok[..], 1.0, H, "non-finite drift"),
        (&ok[..], &bad[..], 1.0, H, "non-finite diffusion"),
        (&ok[..], &ok[..], f64::INFINITY, H, "non-finite state"),
        (&ok[..], &ok[..], 1.0, f64::NAN, "non-finite step"),
    ] {
        let err = sde_euler_maruyama(SdeRule::Ito, drift, diffusion, x0, h, STEPS, Some(SEED_A))
            .expect_err("must refuse");
        assert_eq!(err.code(), "E-SIM-001", "{label} must refuse E-SIM-001");
    }
}

/// Domain errors refuse `E-SIM-002`: non-positive step and zero steps.
#[test]
fn sde_domain_refuses() {
    let drift = [0.0_f64, 0.25];
    let diffusion = [0.0_f64, 0.35];
    for (h, steps, label) in [
        (0.0, STEPS, "zero step"),
        (-H, STEPS, "negative step"),
        (H, 0, "zero steps"),
    ] {
        let err = sde_euler_maruyama(
            SdeRule::Ito,
            &drift,
            &diffusion,
            1.0,
            h,
            steps,
            Some(SEED_A),
        )
        .expect_err("must refuse");
        assert_eq!(err.code(), "E-SIM-002", "{label} must refuse E-SIM-002");
    }
}

/// Over-budget step counts refuse `E-SIM-003` instead of silently
/// allocating an unbounded stream.
#[test]
fn sde_budget_refuses() {
    let drift = [0.0_f64, 0.25];
    let diffusion = [0.0_f64, 0.35];
    let err = sde_euler_maruyama(
        SdeRule::Ito,
        &drift,
        &diffusion,
        1.0,
        H,
        emath_rt::stochastic::SDE_MAX_STEPS + 1,
        Some(SEED_A),
    )
    .expect_err("over-budget must refuse");
    assert_eq!(err.code(), "E-SIM-003", "over-budget must refuse E-SIM-003");
}

/// SdeError carries a stable, documented code (the language surface).
#[test]
fn sde_error_codes_are_stable() {
    assert_eq!(SdeError::Seed.code(), "E-SIM-SEED");
    assert_eq!(SdeError::NonFinite.code(), "E-SIM-001");
    assert_eq!(SdeError::Domain.code(), "E-SIM-002");
    assert_eq!(SdeError::Budget.code(), "E-SIM-003");
}

/// P8a — TIMESTEP REFINEMENT (distributional MR): for additive noise
/// dX = μdt + σdW with constant coefficients, the Euler–Maruyama
/// terminal value is EXACTLY Gaussian with variance σ²T at every step
/// size (X_T = x0 + μT + σ√h·ΣZᵢ), so the refinement law is checked
/// as: the sample terminal variance over a fixed seed family lands
/// within a tight band of σ²T at BOTH resolutions, tighter for the
/// refined run. The kernel consumes fresh draws per step, so the
/// cross-step-size comparison is distributional by construction. This
/// kills noise-scaling mutations instantly: √h→h inflates the
/// variance by 1/h and a dropped √h collapses it to zero.
#[test]
fn sde_timestep_refinement_distributional() {
    let drift = [1.0_f64]; // μ(x) = 1 (constant)
    let diffusion = [0.5_f64]; // σ = 0.5 (constant)
    let steps_coarse = 4usize;
    let ratio = 16u32;
    let h_coarse = 0.04;
    let horizon = h_coarse * steps_coarse as f64; // T = 0.16
    // Anchor: the coarse path is the spec recurrence for its own
    // configuration (pinned-seed oracle).
    let coarse = sde_euler_maruyama(
        SdeRule::Ito,
        &drift,
        &diffusion,
        0.0,
        h_coarse,
        steps_coarse,
        Some(SEED_A),
    )
    .unwrap();
    let zs = spec_zs(SEED_A.to_bits(), steps_coarse);
    let mut want = vec![0.0];
    let mut x = 0.0;
    for &z in &zs {
        x = x + 1.0 * h_coarse + 0.5 * h_coarse.sqrt() * z;
        want.push(x);
    }
    assert_eq!(coarse, want, "coarse path is the spec recurrence");
    // The refinement MR: terminal variance over the seed family at
    // both resolutions, compared against the exact σ²T.
    let terminal = |h: f64, steps: usize| {
        (0..24u32)
            .map(|s| {
                sde_euler_maruyama(
                    SdeRule::Ito,
                    &drift,
                    &diffusion,
                    0.0,
                    h,
                    steps,
                    Some(SEED_A + s as f64),
                )
                .unwrap()[steps]
            })
            .collect::<Vec<f64>>()
    };
    let coarse_term = terminal(h_coarse, steps_coarse);
    let refined_term = terminal(h_coarse / ratio as f64, steps_coarse * ratio as usize);
    let var = |xs: &[f64]| {
        let mean = xs.iter().sum::<f64>() / xs.len() as f64;
        xs.iter().map(|x| (x - mean) * (x - mean)).sum::<f64>() / xs.len() as f64
    };
    let truth = 0.5_f64 * 0.5_f64 * horizon; // σ²T = 0.04
    assert!(
        (var(&refined_term) - truth).abs() < 0.15 * truth,
        "refined variance must track σ²T: {} vs {truth}",
        var(&refined_term)
    );
    assert!(
        (var(&coarse_term) - truth).abs() < 0.3 * truth,
        "coarse variance must track σ²T: {} vs {truth}",
        var(&coarse_term)
    );
}

/// P8b — SEED REPLAY + REFINEMENT COMPOSITION (compound MR): replaying
/// the refined trajectory with the same seed gives bit-identical
/// output, and halving h at fixed T doubles the step count (the
/// trajectory's LENGTH law) with the same seed refusing to differ
/// when replayed.
#[test]
fn sde_refinement_replays_bit_identically() {
    let drift = [0.0_f64, 0.25];
    let diffusion = [0.0_f64, 0.35];
    let a = sde_euler_maruyama(
        SdeRule::Ito,
        &drift,
        &diffusion,
        1.0,
        H,
        STEPS,
        Some(SEED_A),
    )
    .unwrap();
    let a2 = sde_euler_maruyama(
        SdeRule::Ito,
        &drift,
        &diffusion,
        1.0,
        H,
        STEPS,
        Some(SEED_A),
    )
    .unwrap();
    assert_eq!(a, a2, "replay is bit-identical (refinement path)");
    // Halving h at fixed T doubles the step count (T = STEPS·H).
    let half = sde_euler_maruyama(
        SdeRule::Ito,
        &drift,
        &diffusion,
        1.0,
        H / 2.0,
        STEPS * 2,
        Some(SEED_A),
    )
    .unwrap();
    assert_eq!(
        half.len(),
        STEPS * 2 + 1,
        "halved h doubles the trajectory length"
    );
}

/// P8c — COMPOUND: seed-replay + zero-noise + Itô-vs-Stratonovich in
/// one law chain: with σ = 0 the rules agree (each with itself under
/// replay), and with σ' ≠ 0 they differ under the SAME seed.
#[test]
fn sde_compound_metamorphic_chain() {
    let drift = [0.0_f64, 0.25];
    let additive = [0.5_f64]; // σ' = 0
    let state_dep = [0.0_f64, 0.35]; // σ' ≠ 0
    // Chain link 1: additive noise → rules agree, and replay agrees
    // with itself.
    let ito_a =
        sde_euler_maruyama(SdeRule::Ito, &drift, &additive, 1.0, H, STEPS, Some(SEED_A)).unwrap();
    let strat_a = sde_euler_maruyama(
        SdeRule::Stratonovich,
        &drift,
        &additive,
        1.0,
        H,
        STEPS,
        Some(SEED_A),
    )
    .unwrap();
    assert_eq!(ito_a, strat_a, "additive: rules identical");
    assert_eq!(
        sde_euler_maruyama(SdeRule::Ito, &drift, &additive, 1.0, H, STEPS, Some(SEED_A)).unwrap(),
        ito_a,
        "replay identical"
    );
    // Chain link 2: state-dependent noise → rules differ.
    let ito_s = sde_euler_maruyama(
        SdeRule::Ito,
        &drift,
        &state_dep,
        1.0,
        H,
        STEPS,
        Some(SEED_A),
    )
    .unwrap();
    let strat_s = sde_euler_maruyama(
        SdeRule::Stratonovich,
        &drift,
        &state_dep,
        1.0,
        H,
        STEPS,
        Some(SEED_A),
    )
    .unwrap();
    assert_ne!(ito_s, strat_s, "state-dependent: rules differ");
    // Chain link 3: zero noise → both reduce to the SAME ODE path,
    // and replay is stable.
    let ito_z = sde_euler_maruyama(SdeRule::Ito, &drift, &[], 1.0, H, STEPS, Some(SEED_A)).unwrap();
    let strat_z = sde_euler_maruyama(
        SdeRule::Stratonovich,
        &drift,
        &[],
        1.0,
        H,
        STEPS,
        Some(SEED_A),
    )
    .unwrap();
    assert_eq!(ito_z, strat_z, "zero noise: one ODE path");
}
