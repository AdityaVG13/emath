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

/// Single-word gcd and division agree with plain Euclid and the
/// truncating `/` `%` across the i64 and u64 edges, signs, zero, and
/// i128::MIN (whose magnitude leaves the single-word range).
#[test]
fn single_word_gcd_and_division_match_reference_arithmetic() {
    use emath_rt::ExactInt;
    fn euclid(mut a: u128, mut b: u128) -> u128 {
        while b != 0 {
            (a, b) = (b, a % b);
        }
        a
    }
    let mut values: Vec<i128> = vec![
        0,
        1,
        -1,
        2,
        -6,
        12,
        i64::MAX as i128,
        i64::MIN as i128,
        i64::MIN as i128 + 1,
        u64::MAX as i128,
        -(u64::MAX as i128),
        1 << 64,
        3 << 70,
        i128::MAX,
        i128::MIN,
        i128::MIN + 1,
    ];
    let mut seed = 0x9e37_79b9_7f4a_7c15u64;
    for _ in 0..400 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let shared = i128::from(seed % 1000 + 1);
        let wide = (i128::from(seed) << (seed % 60)) >> (seed % 7);
        values.push(wide.wrapping_mul(if seed % 3 == 0 { -shared } else { shared }));
    }
    for &a in &values {
        for &b in values.iter().step_by(7) {
            let g = ExactInt::gcd(&ExactInt::from(a), &ExactInt::from(b)).unwrap();
            assert_eq!(
                g.to_string(),
                euclid(a.unsigned_abs(), b.unsigned_abs()).to_string(),
                "gcd({a}, {b})"
            );
            if b == 0 {
                continue;
            }
            let (q, r) = ExactInt::from(a).div_rem(&ExactInt::from(b)).unwrap();
            match (a.checked_div(b), a.checked_rem(b)) {
                (Some(eq), Some(er)) => {
                    assert_eq!((q, r), (ExactInt::from(eq), ExactInt::from(er)), "{a} / {b}");
                }
                _ => {
                    assert_eq!(q.to_string(), (i128::MAX as u128 + 1).to_string(), "{a} / {b}");
                    assert!(r.is_zero(), "{a} % {b}");
                }
            }
        }
    }
}
