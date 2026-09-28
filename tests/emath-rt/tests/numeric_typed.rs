//! Exact integer carrier ordering across the small/limb storage boundary.

#[test]
fn exact_integer_order_crosses_storage_boundaries() {
    use emath_rt::ExactInt;
    let values = [
        "-340282366920938463463374607431768211456",
        "-170141183460469231731687303715884105729",
        "-170141183460469231731687303715884105728",
        "-4294967296",
        "-1",
        "0",
        "1",
        "4294967296",
        "170141183460469231731687303715884105727",
        "170141183460469231731687303715884105728",
        "340282366920938463463374607431768211456",
    ]
    .map(|text| ExactInt::parse(text).unwrap());
    for (i, a) in values.iter().enumerate() {
        for (j, b) in values.iter().enumerate() {
            assert_eq!(a.cmp(b), i.cmp(&j), "{a} vs {b}");
        }
    }
    for (small, neg, limbs) in [
        (0, false, vec![]),
        (7, false, vec![7]),
        (-7, true, vec![7]),
        (i128::MIN, true, vec![0, 0, 0, 0x8000_0000]),
    ] {
        let a = ExactInt::from(small);
        let b = ExactInt::Big { neg, limbs };
        assert_eq!(a.cmp(&b), std::cmp::Ordering::Equal);
        assert_eq!(b.cmp(&a), std::cmp::Ordering::Equal);
    }
}
