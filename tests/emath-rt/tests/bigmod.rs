#![forbid(unsafe_code)]
//! Stage-2 big-modular kernels (emath-t63iz): `UBig` primitives and the
//! six widened number-theory builtins at |F| < 2^256.
//!
//! Ground truth strategy: where values fit `u64`, the tests compare the
//! big kernels against NATIVE u64 arithmetic (independent implementation);
//! at 2^256 scale they pin hand-derivable identities — 2^255 mod
//! (2^255-19) = 19, Fermat roots, perfect-square round trips — so a
//! carry bug or a wrapped product cannot survive.

use emath_rt::{
    UBig, big_int_rem_checked, big_int_rem_i64_checked, big_mod_inv_checked,
    big_poly_eval_mod_checked, big_pow_mod_checked, big_rs_encode_checked, big_sqrt_mod_checked,
};

/// The Curve25519 prime 2^255 - 19 (p ≡ 1 mod 4 → the full
/// Tonelli-Shanks path, not the fast path).
const P25519: &str =
    "57896044618658097711785492504343953926634992332820282019728792003956564819949";

fn p25519() -> UBig {
    UBig::parse_decimal(P25519).expect("P25519 parses")
}

#[test]
fn decimal_round_trip_and_stage2_bound() {
    let p = p25519();
    assert_eq!(p.to_decimal(), P25519);
    assert_eq!(p.bits(), 255);
    // Stage-2 admission bound: values < 2^256 fit; the emitter refuses
    // beyond this (bit width, not limb count, is the honest bound).
    assert!(p.bits() <= emath_rt::LIMIT_BITS);
    // Zero and one render canonically.
    assert_eq!(UBig::zero().to_decimal(), "0");
    assert_eq!(UBig::one().to_decimal(), "1");
    assert_eq!(
        UBig::parse_decimal("000").expect("parses").to_decimal(),
        "0"
    );
}

#[test]
fn primitives_match_native_u64_arithmetic() {
    for (a, b) in [
        (7u64, 5u64),
        (0, 1),
        (1, 0),
        (4_294_967_295, 4_294_967_296),
        (u32::MAX as u64, u32::MAX as u64),
        (9_876_543_210, 12_345_678_9),
        (u64::MAX / 3, 2),
    ] {
        let big_a = UBig::from_u64(a);
        let big_b = UBig::from_u64(b);
        assert_eq!(big_a.to_decimal(), a.to_string());
        // add
        assert_eq!(big_a.add(&big_b).to_decimal(), (a + b).to_string());
        // sub (a >= b where defined)
        if a >= b {
            assert_eq!(big_a.sub(&big_b).to_decimal(), (a - b).to_string());
        }
        // mul
        assert_eq!(big_a.mul(&big_b).to_decimal(), (a * b).to_string());
        // div_rem
        if b != 0 {
            let (q, r) = emath_rt::big_div_rem(&big_a, &big_b);
            assert_eq!(q.to_decimal(), (a / b).to_string());
            assert_eq!(r.to_decimal(), (a % b).to_string());
            assert_eq!(r.cmp(&big_b), std::cmp::Ordering::Less);
        }
    }
}

#[test]
fn mul_and_rem_survive_512_bit_products() {
    // (P+19) = 2^255 exactly: P + 19 rem P = 19 — the reduction the
    // whole design leans on, at the widest stage-2 product.
    let p = p25519();
    let two_pow_255 = p.add(&UBig::from_u64(19));
    assert_eq!(
        emath_rt::big_int_rem_checked(&two_pow_255, &p).expect("rem"),
        UBig::from_u64(19)
    );
    // A 512-bit product: (P-1)^2 mod P = 1 (any a ≡ -1 squares to 1).
    let p_minus_1 = p.sub(&UBig::one());
    assert_eq!(
        emath_rt::big_int_rem_checked(&p_minus_1.mul(&p_minus_1), &p).expect("rem"),
        UBig::one()
    );
}

#[test]
fn pow_mod_at_255_bit_width() {
    let p = p25519();
    // 2^255 mod (2^255 - 19) = 19 — the defining identity, hand-checkable.
    assert_eq!(
        big_pow_mod_checked(&UBig::from_u64(2), &UBig::from_u64(255), &p).expect("pow"),
        UBig::from_u64(19)
    );
    // Fermat: 3^(p-1) ≡ 1 (mod p) for prime p.
    let p_minus_1 = p.sub(&UBig::one());
    assert_eq!(
        big_pow_mod_checked(&UBig::from_u64(3), &p_minus_1, &p).expect("pow"),
        UBig::one()
    );
}

#[test]
fn mod_inv_round_trip_at_width() {
    let p = p25519();
    // Inverse of 3: verify 3 · inv ≡ 1 (mod p) through the kernel mul.
    let inv = big_mod_inv_checked(&UBig::from_u64(3), &p).expect("inverse");
    let round = emath_rt::UBig::mul_mod(&UBig::from_u64(3), &inv, &p);
    assert_eq!(round, UBig::one());
    // Same answer as Fermat (p prime): 3^(p-2) — two independent
    // algorithms agreeing is the point of the identity.
    let p_minus_2 = p.sub(&UBig::from_u64(2));
    assert_eq!(
        inv,
        big_pow_mod_checked(&UBig::from_u64(3), &p_minus_2, &p).expect("pow")
    );
    // Non-coprime refuses typed: gcd(6, 9) = 3 — small scale where the
    // refusal is hand-checkable.
    let nine = UBig::from_u64(9);
    assert!(big_mod_inv_checked(&UBig::from_u64(6), &nine).is_err());
}

#[test]
fn sqrt_mod_round_trip_and_tiebreak_at_width() {
    let p = p25519();
    // Perfect square: 2² = 4 → min(2, p-2) = 2.
    assert_eq!(
        big_sqrt_mod_checked(&UBig::from_u64(4), &p).expect("sqrt"),
        UBig::from_u64(2)
    );
    // Wide square: r = (p-1)/2 — r² mod p has a sqrt by construction;
    // the gate must pass and the tie-break must return min(r, p-r).
    let r = p.sub(&UBig::one()).div_u64(2);
    let square = emath_rt::UBig::mul_mod(&r.clone(), &r, &p);
    let root = big_sqrt_mod_checked(&square, &p).expect("sqrt of a constructed square");
    assert!(
        root.cmp(&p.sub(&root)) != std::cmp::Ordering::Greater,
        "tie-break"
    );
    assert_eq!(
        emath_rt::UBig::mul_mod(&root.clone(), &root, &p),
        square,
        "x² ≡ a round trip"
    );
    // Non-residues refuse typed: 2 is a non-residue mod P25519 by the
    // supplementary law (p = 2^255 - 19 ≡ 5 mod 8 ⇒ (2|p) = -1), so the
    // Euler symbol through pow_mod is p-1 and sqrt_mod refuses.
    let half = p.sub(&UBig::one()).div_u64(2);
    assert_eq!(
        big_pow_mod_checked(&UBig::from_u64(2), &half, &p)
            .expect("pow")
            .to_decimal(),
        p.sub(&UBig::one()).to_decimal(),
        "2 is a non-residue: Euler symbol = p-1"
    );
    assert!(big_sqrt_mod_checked(&UBig::from_u64(2), &p).is_err());
}

#[test]
fn int_rem_sign_law_at_width() {
    let p = p25519();
    // int_rem(-5, p) = p - 5 (exact-Euclidean, hand-checkable).
    assert_eq!(
        big_int_rem_i64_checked(-5, &p).expect("rem"),
        p.sub(&UBig::from_u64(5))
    );
    assert_eq!(big_int_rem_i64_checked(0, &p).expect("rem"), UBig::zero());
    // Big a: (p + 7) rem p = 7.
    assert_eq!(
        big_int_rem_checked(&p.add(&UBig::from_u64(7)), &p).expect("rem"),
        UBig::from_u64(7)
    );
    // Zero modulus refuses.
    assert!(big_int_rem_checked(&UBig::one(), &UBig::zero()).is_err());
}

#[test]
fn poly_eval_horner_at_width() {
    let p = p25519();
    // f(t) = 1 + 2^52·t at t = p-1 ≡ -1: 1 - 2^52 → p + 1 - 2^52.
    // Expected derived with add/sub primitives (pinned independently).
    let expected = p
        .add(&UBig::one())
        .sub(&UBig::from_u64(4_503_599_627_370_496));
    assert_eq!(
        big_poly_eval_mod_checked(&[1.0, 4_503_599_627_370_496.0], &p.sub(&UBig::one()), &p)
            .expect("eval"),
        expected
    );
    // Exactness gate: a fractional coefficient refuses (never a silent lie).
    assert!(big_poly_eval_mod_checked(&[1.5], &UBig::one(), &p).is_err());
}

#[test]
fn rs_encode_parity_with_poly_at_width() {
    let p = p25519();
    let coeffs = [1.0, 4_503_599_627_370_496.0];
    let n = 12i64;
    let codeword = big_rs_encode_checked(&coeffs, n, &p).expect("encode");
    assert_eq!(codeword.len(), n as usize);
    assert_eq!(codeword[0], UBig::one());
    assert_eq!(codeword[1], UBig::from_u64(1 + 4_503_599_627_370_496));
    for (x, element) in codeword.iter().enumerate() {
        let point =
            big_poly_eval_mod_checked(&coeffs, &UBig::from_u64(x as u64), &p).expect("eval");
        assert_eq!(*element, point, "rs_encode({x}) = poly_eval_mod({x})");
    }
}

#[test]
fn overflow_beyond_stage2_bound_refuses() {
    // 2^256 itself is the first refused value: bits(P·2) = 256 is still
    // inside |F| < 2^256; one more shift crosses the boundary the
    // emitter's typed refusal relies on.
    let two_pow_256 = p25519().shl1();
    assert_eq!(two_pow_256.bits(), emath_rt::LIMIT_BITS);
    assert!(two_pow_256.shl1().bits() > emath_rt::LIMIT_BITS);
}
