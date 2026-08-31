//! `emath-r3-prob-info-2z5e`: B22 information-theory stdlib contract
//! tests.
//!
//! Carrier honesty: probabilities are `f64` with DECLARED validation
//! (non-negative, finite, total mass 1 within `1e-9`) — exact-rational
//! probability is a different contract, never silently swapped in.
//! Entropy and KL are pinned to BITS (Shannon's unit); the nats
//! variant is a separate named function — the base is a declared
//! distinction, never inferred. Differential entropy is a DIFFERENT
//! measure-world contract: its surface refuses `NotImplemented`
//! (giry-probability world pending), which is the discrete-vs-
//! differential type distinction of criterion 4 at the stdlib level.
//!
//! Failure-first: RED (E0432) until `probability` lands.

use emath_core::probability::{
    entropy, entropy_differential, entropy_nats, kl_divergence, mutual_information,
};

const TOL: f64 = 1e-12;

#[test]
fn entropy_fair_coin_is_one_bit() {
    let h = entropy(&[0.5, 0.5]).unwrap();
    assert!((h - 1.0).abs() < TOL, "H(fair coin) = 1 bit, got {h}");
}

#[test]
fn entropy_uniform_three_is_log2_three() {
    let h = entropy(&[1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0]).unwrap();
    assert!((h - 3.0_f64.log2()).abs() < TOL);
}

#[test]
fn entropy_zero_rows_contribute_zero() {
    // The 0·log2(0) := 0 convention: a zero-weight outcome carries no
    // information and must not poison the sum with NaN/−inf.
    let h = entropy(&[0.5, 0.5, 0.0]).unwrap();
    assert!((h - 1.0).abs() < TOL);
}

#[test]
fn entropy_deterministic_is_zero() {
    let h = entropy(&[1.0, 0.0]).unwrap();
    assert!(h.abs() < TOL, "certain outcome has zero entropy, got {h}");
}

#[test]
fn entropy_refuses_invalid_carriers() {
    assert!(entropy(&[-0.5, 1.5]).is_err(), "negative weight refuses");
    assert!(entropy(&[0.5, 0.4]).is_err(), "mass 0.9 ≠ 1 refuses (no silent normalize)");
    assert!(entropy(&[]).is_err(), "empty distribution refuses");
    assert!(entropy(&[f64::NAN, 1.0 - f64::NAN]).is_err(), "non-finite refuses");
}

#[test]
fn entropy_nats_declares_the_base() {
    let bits = entropy(&[0.5, 0.5]).unwrap();
    let nats = entropy_nats(&[0.5, 0.5]).unwrap();
    assert!((nats - 2.0_f64.ln()).abs() < TOL);
    assert!((bits / nats - 2.0_f64.ln().recip()).abs() < TOL, "bits = nats / ln 2");
}

#[test]
fn kl_divergence_identical_is_zero() {
    let d = kl_divergence(&[0.3, 0.7], &[0.3, 0.7]).unwrap();
    assert!(d.abs() < TOL, "D(P‖P) = 0, got {d}");
}

#[test]
fn kl_divergence_known_value() {
    // D(Bern(1/2) ‖ Bern(1/4)) = 1/2·log2(2) + 1/2·log2(2/3) bits.
    let d = kl_divergence(&[0.5, 0.5], &[0.25, 0.75]).unwrap();
    let expected = 0.5 * (0.5_f64 / 0.25).log2() + 0.5 * (0.5_f64 / 0.75).log2();
    assert!((d - expected).abs() < TOL);
}

#[test]
fn kl_support_violation_refuses() {
    // q_i = 0 where p_i > 0 makes KL +∞ — refused by name, never
    // returned as a finite value or NaN.
    let error = kl_divergence(&[0.5, 0.5], &[1.0, 0.0]).unwrap_err();
    assert!(
        error.contains("support"),
        "support violation must be named, got {error}"
    );
}

#[test]
fn kl_divergence_is_asymmetric() {
    // Oracle correction (disclosed): the first draft compared
    // [0.4,0.6] against its swap [0.6,0.4] — but for TWO-element
    // distributions D([a,b]‖[b,a]) = (a−b)·log2(a/b) is EXACTLY
    // symmetric under swap, a degenerate case. A non-degenerate pair
    // (q uniform) pins the true asymmetry.
    let p = [0.25, 0.75];
    let q = [0.5, 0.5];
    let pq = kl_divergence(&p, &q).unwrap();
    let qp = kl_divergence(&q, &p).unwrap();
    assert!((pq - qp).abs() > 1e-6, "KL is not symmetric: {pq} vs {qp}");
}

#[test]
fn mutual_information_independent_is_zero() {
    let joint = vec![vec![0.25, 0.25], vec![0.25, 0.25]];
    let mi = mutual_information(&joint).unwrap();
    assert!(mi.abs() < TOL, "independent factors have MI 0, got {mi}");
}

#[test]
fn mutual_information_perfect_correlation_is_entropy() {
    // Deterministic bijection: I(X;Y) = H(X) = 1 bit.
    let joint = vec![vec![0.5, 0.0], vec![0.0, 0.5]];
    let mi = mutual_information(&joint).unwrap();
    assert!((mi - 1.0).abs() < TOL);
}

#[test]
fn mutual_information_known_positive_value() {
    let joint = vec![vec![0.4, 0.1], vec![0.1, 0.4]];
    let mi = mutual_information(&joint).unwrap();
    assert!((mi - 0.278_071_905_112_637_7).abs() < TOL);
}

#[test]
fn mutual_information_refuses_invalid_joint() {
    assert!(mutual_information(&vec![vec![0.5, 0.5], vec![0.5, 0.6]]).is_err(), "mass 2.1 refuses");
    assert!(mutual_information(&vec![vec![-0.1, 0.6], vec![0.5, 0.0]]).is_err(), "negative cell refuses");
    assert!(mutual_information(&vec![]).is_err(), "empty joint refuses");
    // Sharp ragged pin: flat mass is exactly 1, so ONLY the
    // rectangularity check can catch this carrier.
    let error = mutual_information(&vec![vec![0.5, 0.5], vec![0.5]]).unwrap_err();
    assert!(
        error.contains("ragged"),
        "ragged joint must be named, got {error}"
    );
}

#[test]
fn differential_entropy_is_a_distinct_refusing_contract() {
    // Criterion 4: discrete vs differential is a TYPE/world
    // distinction — the differential surface refuses by name rather
    // than silently reusing the discrete sum (a density integral is
    // not a mass sum; the measure world is the follow-up).
    let error = entropy_differential(&[0.5, 0.5]).unwrap_err();
    assert!(
        error.contains("differential") && error.contains("measure"),
        "differential refusal must name the distinction, got {error}"
    );
}
