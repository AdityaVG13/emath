#![forbid(unsafe_code)]
//! Stage-2 big field carrier (emath-t63iz): `UBig` primitives at
//! |F| < 2^256.

use emath_rt::UBig;
use emath_test_harness::Probe;

/// The Curve25519 prime 2^255 - 19.
const P25519: &str =
    "57896044618658097711785492504343953926634992332820282019728792003956564819949";

fn p25519() -> UBig {
    UBig::parse_decimal(P25519).expect("P25519 parses")
}

#[test]
fn bigmod_kernels() {
    let mut p = Probe::new("UBig matches native u64 and hand identities at 2^255");
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
        });
    }
    p.case("bound", |p| {
        let two_pow_256 = p25519().shl1();
        p.eq("limit", two_pow_256.bits(), emath_rt::LIMIT_BITS);
        p.demand("over", two_pow_256.shl1().bits() > emath_rt::LIMIT_BITS, "2^257 exceeds the bound");
    });
    p.finish();
}
