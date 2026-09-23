#![forbid(unsafe_code)]
//! Stage-2 big-modular kernels (emath-t63iz): `UBig` primitives and the
//! widened builtins at |F| < 2^256.

use emath_rt::{
    UBig, big_int_rem_checked, big_int_rem_i64_checked,
    big_poly_eval_mod_checked, big_pow_mod_checked, big_rs_encode_checked,
};
use emath_test_harness::Probe;

/// The Curve25519 prime 2^255 - 19.
const P25519: &str =
    "57896044618658097711785492504343953926634992332820282019728792003956564819949";

fn p25519() -> UBig {
    UBig::parse_decimal(P25519).expect("P25519 parses")
}

#[test]
fn bigmod_kernels() {
    let mut p = Probe::new("big-mod kernels match native u64 and hand identities at 2^255");
    let prime = p25519();
    p.eq("decimal", prime.to_decimal(), P25519.to_string());
    p.eq("bits", prime.bits(), 255);
    p.demand("bound", prime.bits() <= emath_rt::LIMIT_BITS, "must fit stage-2 bound");
    p.eq("zero", UBig::zero().to_decimal(), "0".to_string());
    p.eq("one", UBig::one().to_decimal(), "1".to_string());
    p.eq(
        "leading-zeros",
        UBig::parse_decimal("000").expect("parses").to_decimal(),
        "0".to_string(),
    );
    for (a, b) in [
        (7u64, 5u64),
        (0, 1),
        (1, 0),
        (4_294_967_295, 4_294_967_296),
        (u32::MAX as u64, u32::MAX as u64),
        (9_876_543_210, 12_345_678_9),
        (u64::MAX / 3, 2),
    ] {
        p.case(&format!("u64/{a}/{b}"), |p| {
            let big_a = UBig::from_u64(a);
            let big_b = UBig::from_u64(b);
            p.eq("dec", big_a.to_decimal(), a.to_string());
            p.eq("add", big_a.add(&big_b).to_decimal(), (a + b).to_string());
            if a >= b {
                p.eq("sub", big_a.sub(&big_b).to_decimal(), (a - b).to_string());
            }
            p.eq("mul", big_a.mul(&big_b).to_decimal(), (a * b).to_string());
            if b != 0 {
                let (q, r) = emath_rt::big_div_rem(&big_a, &big_b);
                p.eq("div", q.to_decimal(), (a / b).to_string());
                p.eq("rem", r.to_decimal(), (a % b).to_string());
                p.eq("rem-lt", r.cmp(&big_b), std::cmp::Ordering::Less);
            }
        });
    }
    p.case("wide-rem", |p| {
        let two_pow_255 = prime.add(&UBig::from_u64(19));
        p.eq(
            "2^255-rem",
            emath_rt::big_int_rem_checked(&two_pow_255, &prime).expect("rem"),
            UBig::from_u64(19),
        );
        let m1 = prime.sub(&UBig::one());
        p.eq(
            "neg1-squared",
            emath_rt::big_int_rem_checked(&m1.mul(&m1), &prime).expect("rem"),
            UBig::one(),
        );
    });
    p.case("pow", |p| {
        p.eq(
            "2^255",
            big_pow_mod_checked(&UBig::from_u64(2), &UBig::from_u64(255), &prime).expect("pow"),
            UBig::from_u64(19),
        );
        let m1 = prime.sub(&UBig::one());
        p.eq(
            "fermat",
            big_pow_mod_checked(&UBig::from_u64(3), &m1, &prime).expect("pow"),
            UBig::one(),
        );
    });
    p.case("int-rem", |p| {
        p.eq("neg", big_int_rem_i64_checked(-5, &prime).expect("rem"), prime.sub(&UBig::from_u64(5)));
        p.eq("zero", big_int_rem_i64_checked(0, &prime).expect("rem"), UBig::zero());
        p.eq(
            "wide",
            big_int_rem_checked(&prime.add(&UBig::from_u64(7)), &prime).expect("rem"),
            UBig::from_u64(7),
        );
        p.demand("zero-mod", big_int_rem_checked(&UBig::one(), &UBig::zero()).is_err(), "zero modulus refuses");
    });
    p.case("poly-rs", |p| {
        let expected = prime.add(&UBig::one()).sub(&UBig::from_u64(4_503_599_627_370_496));
        p.eq(
            "horner",
            big_poly_eval_mod_checked(&[1.0, 4_503_599_627_370_496.0], &prime.sub(&UBig::one()), &prime).expect("eval"),
            expected,
        );
        p.demand("fractional", big_poly_eval_mod_checked(&[1.5], &UBig::one(), &prime).is_err(), "fractional coefficient refuses");
        let coeffs = [1.0, 4_503_599_627_370_496.0];
        let codeword = big_rs_encode_checked(&coeffs, 12, &prime).expect("encode");
        p.eq("rs-len", codeword.len(), 12);
        p.eq("rs-0", codeword[0].clone(), UBig::one());
        p.eq("rs-1", codeword[1].clone(), UBig::from_u64(1 + 4_503_599_627_370_496));
        for (x, element) in codeword.iter().enumerate() {
            p.eq(
                format!("rs[{x}]"),
                element.clone(),
                big_poly_eval_mod_checked(&coeffs, &UBig::from_u64(x as u64), &prime).expect("eval"),
            );
        }
    });
    p.case("bound", |p| {
        let two_pow_256 = p25519().shl1();
        p.eq("limit", two_pow_256.bits(), emath_rt::LIMIT_BITS);
        p.demand("over", two_pow_256.shl1().bits() > emath_rt::LIMIT_BITS, "2^257 exceeds the bound");
    });
    p.finish();
}
