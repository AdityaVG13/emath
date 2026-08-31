//! `emath-r3-numtheory-comb-60ke`: B16 number theory + B17 combinatorics
//! stdlib contract tests.
//!
//! B16 (05 §3.3 #3): deterministic Miller-Rabin primality (u64 carrier),
//! trial-division factorization, gcd/lcm, and the congruence predicate
//! (reference for the admitted EMIR `Congruence` op; C9's Wilson form is
//! `congruence(factorial(p-1), -1, p)`).
//! B17 (05 §3.3 #4): exact-integer counting (i128 carrier, typed overflow
//! refusal — never a silent wrap), `Permutation` with the C10 workaround
//! (`Permutation::new(n)` value ctor; the const-generic `Permutation<8>`
//! is underivable and stays deferred), and lexicographic enumeration
//! under an explicit budget with a resumable continuation.
//!
//! Failure-first: RED (E0432) until `numtheory` / `combinatorics` land.

use emath_core::combinatorics::{binomial, enumerate_from, factorial, Permutation};
use emath_core::numtheory::{congruence, factorize, gcd, is_prime, lcm};

#[test]
fn is_prime_known_primes_and_composites() {
    // Known primes: 2, 3, 97, 7919, 2^31-1, 2^61-1 (Mersenne).
    for n in [2u64, 3, 97, 7919, 2_147_483_647, 2_305_843_009_213_693_951] {
        assert!(is_prime(n), "{n} is prime");
    }
    // Composites: 1 (unit), even, square, Carmichael 561, 2^64-1
    // (= 3·5·17·257·641·65537·6700417).
    for n in [1u64, 4, 9, 561, 18_446_744_073_709_551_615] {
        assert!(!is_prime(n), "{n} is composite");
    }
}

#[test]
fn is_prime_survives_strong_pseudoprimes() {
    // 3,215,031,751 is the smallest strong pseudoprime to bases
    // {2, 3, 5, 7}; 3,825,123,056,546,413,051 to bases 2..=23. A
    // witness-set mutant (dropped witnesses) must fail here.
    assert!(!is_prime(3_215_031_751));
    assert!(!is_prime(3_825_123_056_546_413_051));
}

#[test]
fn factorize_primary_decomposition_reconstructs() {
    let f = factorize(360).unwrap();
    assert_eq!(f.factors, vec![(2, 3), (3, 2), (5, 1)]);
    let product: u64 = f.factors.iter().map(|(p, e)| p.pow(*e)).product();
    assert_eq!(product, 360);
    // Pure power: exponent must fold into one entry, not repeat rows.
    assert_eq!(factorize(1024).unwrap().factors, vec![(2, 10)]);
    // Semiprime with both factors above the small-prime sieve.
    let f = factorize(1_000_003 * 1_000_033).unwrap();
    assert_eq!(f.factors, vec![(1_000_003, 1), (1_000_033, 1)]);
}

#[test]
fn factorize_edges_refuse_or_admit_honestly() {
    assert!(factorize(1).unwrap().factors.is_empty(), "1 is the unit");
    assert!(
        factorize(0).is_err(),
        "0 has no primary decomposition — refuse, never loop"
    );
    // A prime factorizes to itself once (tractable Mersenne 2^31-1).
    assert_eq!(factorize(2_147_483_647).unwrap().factors, vec![(2_147_483_647, 1)]);
}

#[test]
fn gcd_lcm_edges_and_overflow() {
    assert_eq!(gcd(0, 0), 0);
    assert_eq!(gcd(0, 5), 5);
    assert_eq!(gcd(12, 18), 6);
    assert_eq!(gcd(18, 12), 6, "commutative");
    assert_eq!(lcm(4, 6).unwrap(), 12);
    assert_eq!(lcm(0, 7).unwrap(), 0);
    // 3·2^63 exceeds u64 — refuse, never wrap.
    assert!(lcm(1u64 << 63, 3).is_err());
}

#[test]
fn congruence_normalizes_negative_residues() {
    // Wilson at p=5: 4! = 24 ≡ -1 (mod 5) — the C9 respelling target.
    assert!(congruence(24, -1, 5).unwrap());
    assert!(congruence(24, 4, 5).unwrap(), "−1 ≡ 4 (mod 5)");
    assert!(!congruence(24, 1, 5).unwrap());
    assert!(congruence(-1, 4, 5).unwrap(), "negative base normalizes");
    assert!(congruence(6, 0, 3).unwrap());
    assert!(
        congruence(1, 0, 0).is_err(),
        "modulus 0 is meaningless — refuse, never divide by zero"
    );
}

#[test]
fn factorial_exact_until_i128_refusal() {
    assert_eq!(factorial(0).unwrap(), 1);
    assert_eq!(factorial(20).unwrap(), 2_432_902_008_176_640_000);
    assert_eq!(
        factorial(33).unwrap(),
        8_683_317_618_811_886_495_518_194_401_280_000_000
    );
    // 34! overflows i128 — typed refusal, never a wrapped value.
    assert!(factorial(34).is_err());
}

#[test]
fn binomial_exact_with_symmetry_and_overflow() {
    assert_eq!(binomial(52, 5).unwrap(), 2_598_960);
    assert_eq!(binomial(100, 50).unwrap(), 100_891_344_545_564_193_334_812_497_256);
    assert_eq!(
        binomial(12, 5).unwrap(),
        binomial(12, 7).unwrap(),
        "symmetry C(n,k) = C(n,n-k)"
    );
    // Pascal: C(n,k) = C(n-1,k-1) + C(n-1,k) on a spot row.
    assert_eq!(
        binomial(10, 4).unwrap(),
        binomial(9, 3).unwrap() + binomial(9, 4).unwrap()
    );
    // C(200,100) ≈ 9e58 exceeds i128 — refuse.
    assert!(binomial(200, 100).is_err());
}

#[test]
fn permutation_new_is_identity() {
    // C10 workaround: value ctor, not the underivable const-generic.
    let p = Permutation::new(8);
    assert_eq!(p.size(), 8);
    assert_eq!(p.order(), &[0, 1, 2, 3, 4, 5, 6, 7]);
}

#[test]
fn permutation_from_order_validates_bijection() {
    assert!(Permutation::from_order(&[2, 0, 1]).is_ok());
    assert!(Permutation::from_order(&[0, 0]).is_err(), "duplicate");
    assert!(Permutation::from_order(&[0, 2]).is_err(), "out of range");
}

#[test]
fn permutation_apply_permutes_by_index() {
    let p = Permutation::from_order(&[2, 0, 1]).unwrap();
    let source = [10, 20, 30];
    let applied: Vec<i32> = (0..3).map(|i| source[p.apply(i) as usize]).collect();
    assert_eq!(applied, vec![30, 10, 20]);
}

#[test]
fn permutation_next_walks_lexicographic_and_terminates() {
    let p = Permutation::new(3);
    let first = p.successor().unwrap();
    assert_eq!(first.order(), &[0, 2, 1], "first successor");
    // The last permutation of 0..n has no successor.
    let last = Permutation::from_order(&[2, 1, 0]).unwrap();
    assert!(last.successor().is_none(), "exhausted: None, never wraps");
}

#[test]
fn enumerate_budget_and_continuation_resume_exactly() {
    let (batch, cont) = enumerate_from(Permutation::new(4), 10);
    assert_eq!(batch.len(), 10);
    assert!(cont.is_some(), "24 total > 10 budget: continuation exists");
    let (second, cont2) = enumerate_from(cont.unwrap(), 10);
    assert_eq!(second.len(), 10);
    assert!(cont2.is_some());
    let (third, done) = enumerate_from(cont2.unwrap(), 10);
    assert_eq!(third.len(), 4, "24 = 10 + 10 + 4, no duplicates, no loss");
    assert!(done.is_none(), "exhaustion is the named continuation end");
    // No element repeats across the three batches.
    let mut seen: Vec<Vec<u32>> = Vec::new();
    seen.extend(batch.iter().chain(second.iter()).chain(third.iter()).map(|p| p.order().to_vec()));
    seen.sort();
    seen.dedup();
    assert_eq!(seen.len(), 24);
}
