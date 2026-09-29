//! Exact integer carrier ordering across the small/limb storage boundary,
//! and multi-limb division.

/// `n = q * d + r` with `|r| < |d|` and `r` carrying the sign of `n`
/// pins the truncating quotient uniquely. Limbs lean on 0, 1, 2^31 and
/// 2^32 - 1, where quotient-limb estimates overshoot; the first case is
/// a known add-back vector (Hacker's Delight divmnu, scaled by 2^128).
#[test]
fn multi_limb_division_meets_its_identity() {
    use emath_rt::ExactInt;
    let big = |neg: bool, limbs: Vec<u32>| ExactInt::Big { neg, limbs };
    let mut cases = vec![(
        big(false, vec![0, 0, 0, 0, 0, 0, 0x8000_0000, 0x7fff_ffff]),
        big(false, vec![0, 0, 0, 0, 1, 0, 0x8000_0000]),
    )];
    let mut seed = 0x2545_f491_4f6c_dd1du64;
    let mut draw = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let limbs = |len: usize, draw: &mut dyn FnMut() -> u64| {
        let mut out: Vec<u32> = (0..len)
            .map(|_| match draw() % 6 {
                0 => 0,
                1 => 1,
                2 => 0x8000_0000,
                3 => u32::MAX,
                _ => draw() as u32,
            })
            .collect();
        if out[len - 1] == 0 {
            out[len - 1] = 1 + (draw() as u32 >> 1);
        }
        out
    };
    for _ in 0..3000 {
        let dl = 2 + (draw() % 9) as usize;
        let nl = 5.max(dl) + (draw() % 9) as usize;
        let n = limbs(nl, &mut draw);
        let d = limbs(dl, &mut draw);
        cases.push((big(draw() % 2 == 0, n), big(draw() % 2 == 0, d)));
    }
    for (n, d) in &cases {
        let (q, r) = n.div_rem(d).unwrap();
        assert_eq!(q.mul(d).unwrap().add(&r).unwrap(), *n, "{n} / {d}");
        assert!(r.abs() < d.abs(), "{n} / {d}: remainder {r}");
        assert!(r.is_zero() || r.is_negative() == n.is_negative(), "{n} / {d}: sign of {r}");
    }
}

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
