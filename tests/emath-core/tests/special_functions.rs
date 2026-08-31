//! `emath-r3-spec-funcs-s54f`: core::special_functions (05 section 3.3
//! #1, Phase 11) — contract tests for the strict-f64 reference impls.
//!
//! Every implemented function carries an EXPLICIT numeric error bound
//! (never "accurate"); the pins verify (a) known values within the
//! declared bound, (b) domain/branch refusals named, (c) the bound
//! actually COVERING the error against independently-known reference
//! values, and (d) the provider contract shape. No-claim: nothing here
//! asserts correctly-rounded results where not proven so.
//!
//! Failure-first: this file is RED until `emath_core::special` lands
//! (unresolved module = recorded red).

use emath_core::special::{
    DomainRefusal, SpecialFn, SpecialFunctionEvaluator, StrictF64Reference,
};

const SQRT_PI: f64 = 1.7724538509055160272981674883411;
const PI: f64 = std::f64::consts::PI;

#[test]
fn gamma_known_values_within_bound() {
    let evaluator = StrictF64Reference;
    // Γ(n) = (n−1)!
    for (z, expected) in [(1.0, 1.0), (5.0, 24.0), (10.0, 362880.0)] {
        let evaluated = evaluator
            .evaluate(SpecialFn::Gamma, &[z])
            .expect("gamma at positive integer");
        assert!(
            (evaluated.value - expected).abs() <= evaluated.error_bound,
            "Γ({z}) = {} not within {} of {expected}",
            evaluated.value,
            evaluated.error_bound
        );
        // Γ(1/2) = √π
        let half = evaluator.evaluate(SpecialFn::Gamma, &[0.5]).unwrap();
        assert!(
            (half.value - SQRT_PI).abs() <= half.error_bound,
            "Γ(1/2) off: {} vs {SQRT_PI}",
            half.value
        );
    }
}

#[test]
fn gamma_reflection_branch_agrees() {
    // The reflection branch (z < 0.5, non-integer) must agree with the
    // known value: Γ(−1.5) = 4√π/3 ≈ 2.3632718.
    let evaluator = StrictF64Reference;
    let evaluated = evaluator.evaluate(SpecialFn::Gamma, &[-1.5]).unwrap();
    let expected = 4.0 * SQRT_PI / 3.0;
    assert!(
        (evaluated.value - expected).abs() <= evaluated.error_bound,
        "Γ(−1.5) = {} not within {} of {expected}",
        evaluated.value,
        evaluated.error_bound
    );
}

#[test]
fn gamma_poles_refuse_named() {
    // Γ has simple poles at 0, −1, −2, ... — refused, named as poles.
    let evaluator = StrictF64Reference;
    for z in [0.0, -1.0, -3.0] {
        let error = evaluator.evaluate(SpecialFn::Gamma, &[z]).unwrap_err();
        assert!(
            matches!(error, DomainRefusal::Pole { .. }),
            "Γ({z}) must refuse as a pole, got {error:?}"
        );
    }
}

#[test]
fn beta_known_value() {
    // B(2, 3) = Γ(2)Γ(3)/Γ(5) = 1·2/24 = 1/12.
    let evaluator = StrictF64Reference;
    let evaluated = evaluator.evaluate(SpecialFn::Beta, &[2.0, 3.0]).unwrap();
    assert!(
        (evaluated.value - 1.0 / 12.0).abs() <= evaluated.error_bound,
        "B(2,3) off: {} vs {}",
        evaluated.value,
        1.0 / 12.0
    );
}

#[test]
fn erf_series_matches_known_values_and_bound_covers() {
    let evaluator = StrictF64Reference;
    // erf(1) = 0.84270079294971486934122063508261 (independently known).
    let one = evaluator.evaluate(SpecialFn::Erf, &[1.0]).unwrap();
    assert!(
        (one.value - 0.8427007929497149).abs() <= one.error_bound,
        "erf(1) off: {} ± {}",
        one.value,
        one.error_bound
    );
    assert!(
        one.error_bound < 1e-12,
        "series bound must be certified-small, got {}",
        one.error_bound
    );
    // Odd function: erf(−1) = −erf(1) (bound-aware).
    let neg = evaluator.evaluate(SpecialFn::Erf, &[-1.0]).unwrap();
    assert!((neg.value + one.value).abs() <= neg.error_bound + one.error_bound);
    // Large argument: tail bound covers the deviation from 1.
    let big = evaluator.evaluate(SpecialFn::Erf, &[5.0]).unwrap();
    assert!((big.value - 1.0).abs() <= big.error_bound, "erf(5) bound must cover 1−erf(5)");
}

#[test]
fn zeta_known_values_within_bound() {
    let evaluator = StrictF64Reference;
    // ζ(2) = π²/6, ζ(4) = π⁴/90, ζ(3) ≈ 1.202056903159594.
    let two = evaluator.evaluate(SpecialFn::Zeta, &[2.0]).unwrap();
    assert!(
        (two.value - PI * PI / 6.0).abs() <= two.error_bound,
        "ζ(2) off: {} ± {}",
        two.value,
        two.error_bound
    );
    let three = evaluator.evaluate(SpecialFn::Zeta, &[3.0]).unwrap();
    assert!(
        (three.value - 1.2020569031595942).abs() <= three.error_bound,
        "ζ(3) off: {} ± {}",
        three.value,
        three.error_bound
    );
}

#[test]
fn zeta_pole_and_carrier_refuse() {
    let evaluator = StrictF64Reference;
    // Pole at s = 1; the reference carrier is real s > 1.
    assert!(matches!(
        evaluator.evaluate(SpecialFn::Zeta, &[1.0]).unwrap_err(),
        DomainRefusal::Pole { .. }
    ));
    assert!(matches!(
        evaluator.evaluate(SpecialFn::Zeta, &[0.5]).unwrap_err(),
        DomainRefusal::OutsideCarrier { .. }
    ));
}

#[test]
fn lambert_w0_known_values() {
    let evaluator = StrictF64Reference;
    // W(1) = Ω (omega constant), W(e) = 1, W(2e²) = 2, W(0) = 0.
    // (Oracle correction: W(e²) ≈ 1.5571, NOT 2 — the classic identity
    // is W(2e²) = 2 since 2·e² = 2e².)
    for (z, expected) in [
        (1.0, 0.5671432904097838),
        (std::f64::consts::E, 1.0),
        (2.0 * std::f64::consts::E * std::f64::consts::E, 2.0),
        (0.0, 0.0),
    ] {
        let evaluated = evaluator.evaluate(SpecialFn::LambertW0, &[z]).unwrap();
        assert!(
            (evaluated.value - expected).abs() <= evaluated.error_bound,
            "W₀({z}) = {} not within {} of {expected}",
            evaluated.value,
            evaluated.error_bound
        );
    }
}

#[test]
fn lambert_w0_branch_domain_refuses() {
    // W₀'s real branch is z >= −1/e; below it refuses naming the branch
    // cut (principal branch named, never implicit).
    let evaluator = StrictF64Reference;
    assert!(matches!(
        evaluator.evaluate(SpecialFn::LambertW0, &[-1.0]).unwrap_err(),
        DomainRefusal::OutsideCarrier { .. }
    ));
    // Boundary point is admitted: W₀(−1/e) = −1 (exact special case).
    let boundary = evaluator
        .evaluate(SpecialFn::LambertW0, &[-std::f64::consts::E.recip()])
        .unwrap();
    assert!((boundary.value - (-1.0)).abs() <= boundary.error_bound);
}

#[test]
fn elliptic_integrals_known_values() {
    let evaluator = StrictF64Reference;
    // K(0) = E(0) = π/2; K(1/2) ≈ 1.8540746773013719; E(1/2) ≈ 1.3506438810476762.
    let k0 = evaluator.evaluate(SpecialFn::EllipticK, &[0.0]).unwrap();
    assert!((k0.value - PI / 2.0).abs() <= k0.error_bound);
    let e0 = evaluator.evaluate(SpecialFn::EllipticE, &[0.0]).unwrap();
    assert!((e0.value - PI / 2.0).abs() <= e0.error_bound);
    let kh = evaluator.evaluate(SpecialFn::EllipticK, &[0.5]).unwrap();
    assert!(
        (kh.value - 1.8540746773013719).abs() <= kh.error_bound,
        "K(1/2) off: {} ± {}",
        kh.value,
        kh.error_bound
    );
    let eh = evaluator.evaluate(SpecialFn::EllipticE, &[0.5]).unwrap();
    assert!(
        (eh.value - 1.3506438810476762).abs() <= eh.error_bound,
        "E(1/2) off: {} ± {}",
        eh.value,
        eh.error_bound
    );
}

#[test]
fn elliptic_domain_refuses_named() {
    // The carrier is the parameter m ∈ [0, 1); m = 1 diverges (K), m
    // outside [0, 1) is outside the declared real carrier.
    let evaluator = StrictF64Reference;
    assert!(matches!(
        evaluator.evaluate(SpecialFn::EllipticK, &[1.0]).unwrap_err(),
        DomainRefusal::OutsideCarrier { .. }
    ));
    assert!(matches!(
        evaluator.evaluate(SpecialFn::EllipticE, &[-0.5]).unwrap_err(),
        DomainRefusal::OutsideCarrier { .. }
    ));
}

#[test]
fn provider_contract_shape() {
    // The provider trait is object-safe and returns value + bound; the
    // strict-f64 reference is one implementation of it (backends behind
    // the contract, never baked into core semantics).
    let backend: Box<dyn SpecialFunctionEvaluator> = Box::new(StrictF64Reference);
    let evaluated = backend.evaluate(SpecialFn::Gamma, &[5.0]).unwrap();
    assert_eq!(evaluated.value, 24.0);
    assert!(evaluated.error_bound.is_finite() && evaluated.error_bound >= 0.0);
}

#[test]
fn bounds_actually_cover_error() {
    // The no-claim discipline made checkable: against independent
    // high-precision references (hard-coded constants here), the
    // DECLARED bound must cover the true deviation — an under-stated
    // bound is the same lie as no bound.
    let evaluator = StrictF64Reference;
    let checks: [(SpecialFn, Vec<f64>, f64); 4] = [
        (SpecialFn::Gamma, vec![0.5], SQRT_PI),
        (SpecialFn::Erf, vec![1.0], 0.84270079294971486934),
        (SpecialFn::Zeta, vec![2.0], PI * PI / 6.0),
        (SpecialFn::LambertW0, vec![1.0], 0.56714329040978387299997),
    ];
    for (function, args, reference) in checks {
        let evaluated = evaluator.evaluate(function, &args).unwrap();
        let true_error = (evaluated.value - reference).abs();
        assert!(
            true_error <= evaluated.error_bound,
            "{function:?}{args:?}: declared bound {} does not cover true error {true_error}",
            evaluated.error_bound
        );
    }
}
