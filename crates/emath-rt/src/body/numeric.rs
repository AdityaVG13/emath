// ── Number theory / finite-field arithmetic ───────────────────────────────

/// Euclid over unsigned magnitudes; gcd(0, 0) = 0 by the
/// divisibility-lattice convention (0 divides only 0, and gcd is the
/// lattice meet). The one refusal is the 2^63 magnitude (|i64::MIN|)
/// paired with 0, whose gcd has no i64 carrier. Codegen parity twin of
/// the interpreter's `euclidean_gcd` handler (exec-ir native_kernel.rs).
pub fn gcd_checked(a: i64, b: i64) -> Result<i64, &'static str> {
    let mut left = u128::from(a.unsigned_abs());
    let mut right = u128::from(b.unsigned_abs());
    while right != 0 {
        (left, right) = (right, left % right);
    }
    i64::try_from(left)
        .map_err(|_| "E-ARITH-OVERFLOW: euclidean-gcd result exceeds the i64 carrier")
}

/// Least common multiple: lcm(0, x) = 0; otherwise |a|/gcd · |b| in u128
/// intermediates (|a|, |b| <= 2^63, so the widened product cannot wrap
/// u128), and a result past i64::MAX refuses typed instead of wrapping.
/// Codegen parity twin of the interpreter's `checked_lcm` handler.
pub fn lcm_checked(a: i64, b: i64) -> Result<i64, &'static str> {
    let left = u128::from(a.unsigned_abs());
    let right = u128::from(b.unsigned_abs());
    if left == 0 || right == 0 {
        return Ok(0);
    }
    let mut x = left;
    let mut y = right;
    while y != 0 {
        (x, y) = (y, x % y);
    }
    let lcm = left / x * right;
    i64::try_from(lcm)
        .map_err(|_| "E-ARITH-OVERFLOW: checked-lcm overflowed the i64 carrier")
}

/// Euclidean remainder of `value` modulo a positive `modulus`.
/// Codegen parity twin of the interpreter's `integer_remainder` handler
/// (i64 carrier route).
pub fn int_rem_checked(value: i64, modulus: i64) -> Result<i64, &'static str> {
    if modulus <= 0 {
        return Err("int-rem: modulus must be positive");
    }
    Ok(value.rem_euclid(modulus))
}

/// Congruence of two exact integers modulo a non-zero modulus (i128
/// intermediates so `rem_euclid` cannot overflow). Codegen parity twin
/// of the interpreter's `modular_congruence` handler (i64 route).
pub fn congruence_checked(left: i64, right: i64, modulus: i64) -> Result<bool, &'static str> {
    if modulus == 0 {
        return Err("cong: modulus must be non-zero");
    }
    Ok(i128::from(left).rem_euclid(i128::from(modulus))
        == i128::from(right).rem_euclid(i128::from(modulus)))
}

/// Factorial of n in [0, 20] (i64 range; panics outside).
pub fn factorial(n: i64) -> i64 {
    match factorial_checked(n) {
        Ok(v) => v,
        Err(e) => panic!("{e}"),
    }
}

/// Factorial with a typed error instead of a panic.
pub fn factorial_checked(n: i64) -> Result<i64, &'static str> {
    if !(0..=20).contains(&n) {
        return Err("factorial overflow: n must be in [0, 20] for i64");
    }
    Ok((1..=n).fold(1i64, |acc, k| acc * k))
}

/// Multiplicative inverse of `a` modulo `m` (panics when the modulus is
/// non-positive or no inverse exists).
pub fn mod_inv(a: i64, m: i64) -> i64 {
    match mod_inv_checked(a, m) {
        Ok(v) => v,
        Err(e) => panic!("{e}"),
    }
}

/// Multiplicative inverse with a typed error instead of a panic.
pub fn mod_inv_checked(a: i64, m: i64) -> Result<i64, &'static str> {
    if m <= 0 {
        return Err("mod_inv: modulus must be positive");
    }
    let (g, x, _) = extended_gcd(a.rem_euclid(m), m);
    if g != 1 {
        return Err("mod_inv: no inverse exists (gcd != 1)");
    }
    Ok(x.rem_euclid(m))
}

/// Modular exponentiation `base^exp mod m` via square-and-multiply
/// (panics on `m <= 0` or a negative exponent).
pub fn pow_mod(base: i64, exp: i64, m: i64) -> i64 {
    match pow_mod_checked(base, exp, m) {
        Ok(v) => v,
        Err(e) => panic!("{e}"),
    }
}

/// Modular exponentiation with a typed error instead of a panic.
/// Square-and-multiply over i128 intermediates: with `m <= 2^63` the
/// widest product is `< 2^126`, so i64 operands never overflow the
/// intermediate product (the naive `int_rem(base.pow(exp), m)` would).
pub fn pow_mod_checked(base: i64, exp: i64, m: i64) -> Result<i64, &'static str> {
    if m <= 0 {
        return Err("pow_mod: modulus must be positive");
    }
    if exp < 0 {
        return Err("pow_mod: exponent must be non-negative");
    }
    let modulus: i128 = m as i128;
    let mut result: i128 = 1 % modulus;
    let mut b: i128 = (base as i128).rem_euclid(modulus);
    let mut e = exp as u64;
    while e > 0 {
        if e & 1 == 1 {
            result = (result * b) % modulus;
        }
        b = (b * b) % modulus;
        e >>= 1;
    }
    Ok(result as i64)
}

/// Modular square root in F_p via Tonelli-Shanks (panics on an invalid
/// modulus or a non-residue).
pub fn sqrt_mod(a: i64, p: i64) -> i64 {
    match sqrt_mod_checked(a, p) {
        Ok(v) => v,
        Err(e) => panic!("{e}"),
    }
}

/// Modular square root with a typed error instead of a panic.
/// Returns `x` with `x² ≡ a (mod p)` for prime `p`; refuses typed when
/// `a` is a quadratic non-residue (mirrors `mod_inv`'s refusal style),
/// `p <= 0`, or `p` is even and > 2. Deterministic tie-break: returns
/// `min(x, p - x)`. i128 intermediates keep products exact for `p` up
/// to 2^63.
pub fn sqrt_mod_checked(a: i64, p: i64) -> Result<i64, &'static str> {
    if p <= 0 {
        return Err("sqrt_mod: modulus must be positive");
    }
    if p == 2 {
        return Ok(a.rem_euclid(2));
    }
    if p % 2 == 0 {
        return Err("sqrt_mod: modulus must be an odd prime (2 handled above)");
    }
    let modulus: i128 = p as i128;
    let root_candidate: i128 = (a as i128).rem_euclid(modulus);
    if root_candidate == 0 {
        return Ok(0);
    }
    // Fast path: p ≡ 3 (mod 4) → x = a^((p+1)/4).
    let mut x: i128 = if p % 4 == 3 {
        pow_mod_i128(root_candidate, ((p + 1) / 4) as u64, modulus)
    } else {
        // Legendre pre-check (found by the emath-t63iz wide-mod tests):
        // the Tonelli-Shanks loop below assumes `a` is a residue — for a
        // non-residue the least-i search reaches i = m and the shift
        // m - i - 1 underflows. Refuse here; the exactness gate below
        // still backstops non-prime p.
        if pow_mod_i128(root_candidate, ((p - 1) / 2) as u64, modulus) != 1 {
            return Err("sqrt_mod: no square root exists (a is a non-residue or p is not prime)");
        }
        // General Tonelli-Shanks: p - 1 = q·2^s with q odd.
        let mut q = (p - 1) / 2;
        let mut s: u32 = 1;
        while q % 2 == 0 {
            q /= 2;
            s += 1;
        }
        // Deterministic non-residue search (smallest z whose Legendre
        // symbol is -1; always exists for prime p).
        let mut z = 2i64;
        loop {
            if pow_mod_i128((z as i128).rem_euclid(modulus), ((p - 1) / 2) as u64, modulus)
                == modulus - 1
            {
                break;
            }
            z += 1;
        }
        let mut m = s as u64;
        let mut c = pow_mod_i128(z as i128, q as u64, modulus);
        let mut t = pow_mod_i128(root_candidate, q as u64, modulus);
        let mut r = pow_mod_i128(root_candidate, ((q + 1) / 2) as u64, modulus);
        while t != 1 {
            // Least i with t^(2^i) = 1.
            let mut i: u64 = 0;
            let mut tt = t;
            while tt != 1 {
                tt = (tt * tt) % modulus;
                i += 1;
            }
            let b = pow_mod_i128(c, 1u64 << (m - i - 1), modulus);
            m = i;
            c = (b * b) % modulus;
            t = (t * c) % modulus;
            r = (r * b) % modulus;
        }
        r
    };
    // Defensive exactness gate: a fabricated root must never escape
    // (this is also the typed refusal path for quadratic non-residues).
    if (x * x) % modulus != root_candidate {
        return Err("sqrt_mod: no square root exists (a is a non-residue or p is not prime)");
    }
    if x > modulus - x {
        x = modulus - x;
    }
    Ok(x as i64)
}

/// i128 square-and-multiply (shared by the sqrt_mod paths).
fn pow_mod_i128(base: i128, exp: u64, modulus: i128) -> i128 {
    let mut result: i128 = 1 % modulus;
    let mut b = base;
    let mut e = exp;
    while e > 0 {
        if e & 1 == 1 {
            result = (result * b) % modulus;
        }
        b = (b * b) % modulus;
        e >>= 1;
    }
    result
}

/// Evaluate c[0] + c[1]x + ... + c[k-1]x^(k-1) over GF(p) by Horner's
/// method (panics when the modulus is non-positive).
pub fn poly_eval_mod(coeffs: &[f64], x: i64, p: i64) -> i64 {
    match poly_eval_mod_checked(coeffs, x, p) {
        Ok(v) => v,
        Err(e) => panic!("{e}"),
    }
}

/// Polynomial evaluation over GF(p) with a typed error instead of a panic.
pub fn poly_eval_mod_checked(coeffs: &[f64], x: i64, p: i64) -> Result<i64, &'static str> {
    if p <= 0 {
        return Err("poly_eval_mod: modulus must be positive");
    }
    horner_mod_i128(coeffs, x, p)
}

/// Shared Horner kernel over i128 intermediates (emath-t63iz stage 1):
/// with `p ≤ 2^63` the widest step is `result·x + c < 2^126 + 2^63`,
/// exact in i128 — the same width contract as `pow_mod`/`sqrt_mod`.
/// An i64 product here silently wraps (or panics in debug) for `p` past
/// ~3e9; the wide-modulus tests pin exactness at p = 2^61 - 1.
fn horner_mod_i128(coeffs: &[f64], x: i64, p: i64) -> Result<i64, &'static str> {
    let modulus: i128 = p as i128;
    let point: i128 = x as i128;
    let mut result: i128 = 0;
    for &c in coeffs.iter().rev() {
        result = (result * point + exact_i64(c)? as i128).rem_euclid(modulus);
    }
    Ok(result as i64)
}

/// Reed-Solomon codeword: polynomial evaluation at x = 0..n over GF(p)
/// (panics on an invalid modulus or codeword length).
pub fn rs_encode(coeffs: &[f64], n: i64, p: i64) -> Vec<f64> {
    match rs_encode_checked(coeffs, n, p) {
        Ok(v) => v,
        Err(e) => panic!("{e}"),
    }
}

/// Reed-Solomon codeword with a typed error instead of a panic.
pub fn rs_encode_checked(coeffs: &[f64], n: i64, p: i64) -> Result<Vec<f64>, &'static str> {
    if p <= 0 {
        return Err("rs_encode: modulus must be positive");
    }
    if n <= 0 || n as usize > p as usize {
        return Err("rs_encode: codeword length n must be in (0, p]");
    }
    let mut codeword = Vec::with_capacity(n as usize);
    for x in 0..n {
        codeword.push(horner_mod_i128(coeffs, x, p)? as f64);
    }
    Ok(codeword)
}

/// `as i64` maps NaN→0, Inf→saturating extremes, and |x|<1 (including
/// subnormals)→0. Integer kernels refuse that silent finite lie.
fn exact_i64(value: f64) -> Result<i64, &'static str> {
    if !value.is_finite() || value.fract() != 0.0 {
        return Err("coefficient must be a finite whole number");
    }
    if value < i64::MIN as f64 || value > i64::MAX as f64 {
        return Err("coefficient exceeds i64 range");
    }
    Ok(value as i64)
}

/// Exact mixed `Int` vs `Float64` compare. Widening `n as f64` is a lie
/// past 2^53: `((1<<53)+1) as f64 == (1<<53) as f64`. Returns `None` for
/// NaN (IEEE unordered). `+0`/`-0` compare equal. `i64::MAX as f64` is
/// 2^63 (outside i64), so the bound is `2^63`, not `i64::MAX as f64`.
pub fn cmp_i64_f64(n: i64, x: f64) -> Option<core::cmp::Ordering> {
    if x.is_nan() {
        return None;
    }
    if x == f64::INFINITY {
        return Some(core::cmp::Ordering::Less);
    }
    if x == f64::NEG_INFINITY {
        return Some(core::cmp::Ordering::Greater);
    }
    // First f64 integer outside i64. `i64::MAX as f64` *is* this value.
    const TWO_POW_63: f64 = 9_223_372_036_854_775_808.0;
    let trunc = x.trunc();
    if trunc < i64::MIN as f64 {
        return Some(core::cmp::Ordering::Greater);
    }
    if trunc >= TWO_POW_63 {
        return Some(core::cmp::Ordering::Less);
    }
    let xi = trunc as i64;
    match n.cmp(&xi) {
        core::cmp::Ordering::Equal if x == trunc => Some(core::cmp::Ordering::Equal),
        core::cmp::Ordering::Equal if x > 0.0 => Some(core::cmp::Ordering::Less),
        core::cmp::Ordering::Equal => Some(core::cmp::Ordering::Greater),
        other => Some(other),
    }
}

/// Exact mixed equality; `false` for NaN (IEEE `==`).
pub fn eq_i64_f64(n: i64, x: f64) -> bool {
    matches!(cmp_i64_f64(n, x), Some(core::cmp::Ordering::Equal))
}

/// Hamming distance between two equal-length vectors (panics on length
/// mismatch). Equality is bit-exact (`to_bits`).
pub fn hamming_distance(a: &[f64], b: &[f64]) -> i64 {
    match hamming_distance_checked(a, b) {
        Ok(v) => v,
        Err(e) => panic!("{e}"),
    }
}

/// Hamming distance with a typed error instead of a panic.
pub fn hamming_distance_checked(a: &[f64], b: &[f64]) -> Result<i64, &'static str> {
    if a.len() != b.len() {
        return Err("hamming_distance: vectors must have equal length");
    }
    Ok(a.iter()
        .zip(b.iter())
        .filter(|(x, y)| x.to_bits() != y.to_bits())
        .count() as i64)
}

// ── Higher-order drivers ──────────────────────────────────────────────────

/// Fold an integer range with `+` and an f64 accumulator.
pub fn fold_add(f: &impl Fn(f64) -> f64, start: i64, end: i64, init: f64) -> f64 {
    let mut acc = init;
    for i in start..end {
        acc += f(i as f64);
    }
    acc
}

/// Fold an integer range with `*` and an f64 accumulator.
pub fn fold_mul(f: &impl Fn(f64) -> f64, start: i64, end: i64, init: f64) -> f64 {
    let mut acc = init;
    for i in start..end {
        acc *= f(i as f64);
    }
    acc
}

/// Fold an integer range with `+` and an exact i64 accumulator.
/// Panics on overflow so generated code matches interp's named i64 fault.
pub fn fold_add_i64(f: &impl Fn(i64) -> i64, start: i64, end: i64, init: i64) -> i64 {
    let mut acc = init;
    for i in start..end {
        acc = acc.checked_add(f(i)).expect("i64 overflow");
    }
    acc
}

/// Fold an integer range with `*` and an exact i64 accumulator.
/// Panics on overflow so generated code matches interp's named i64 fault.
pub fn fold_mul_i64(f: &impl Fn(i64) -> i64, start: i64, end: i64, init: i64) -> i64 {
    let mut acc = init;
    for i in start..end {
        acc = acc.checked_mul(f(i)).expect("i64 overflow");
    }
    acc
}

/// Forall over an integer range (AND-combined predicate).
pub fn fold_all(f: &impl Fn(f64) -> bool, start: i64, end: i64, init: bool) -> bool {
    let mut acc = init;
    for i in start..end {
        acc &= f(i as f64);
    }
    acc
}

/// Exists over an integer range (OR-combined predicate).
pub fn fold_any(f: &impl Fn(f64) -> bool, start: i64, end: i64, init: bool) -> bool {
    let mut acc = init;
    for i in start..end {
        acc |= f(i as f64);
    }
    acc
}

/// `fold_add` that propagates a body's typed index fault.
pub fn fold_add_checked(
    f: &impl Fn(f64) -> Result<f64, String>,
    start: i64,
    end: i64,
    init: f64,
) -> Result<f64, String> {
    let mut acc = init;
    for i in start..end {
        acc += f(i as f64)?;
    }
    Ok(acc)
}

/// `fold_mul` that propagates a body's typed index fault.
pub fn fold_mul_checked(
    f: &impl Fn(f64) -> Result<f64, String>,
    start: i64,
    end: i64,
    init: f64,
) -> Result<f64, String> {
    let mut acc = init;
    for i in start..end {
        acc *= f(i as f64)?;
    }
    Ok(acc)
}

/// `fold_all` that propagates a body's typed index fault.
pub fn fold_all_checked(
    f: &impl Fn(f64) -> Result<bool, String>,
    start: i64,
    end: i64,
    init: bool,
) -> Result<bool, String> {
    let mut acc = init;
    for i in start..end {
        acc &= f(i as f64)?;
    }
    Ok(acc)
}

/// `fold_any` that propagates a body's typed index fault.
pub fn fold_any_checked(
    f: &impl Fn(f64) -> Result<bool, String>,
    start: i64,
    end: i64,
    init: bool,
) -> Result<bool, String> {
    let mut acc = init;
    for i in start..end {
        acc |= f(i as f64)?;
    }
    Ok(acc)
}

/// Composite Simpson's rule quadrature over an even positive panel count
/// (panics otherwise). Mirrors the historical inline order: h = (b-a)/n,
/// weights 1/4/2.../4/1, acc * h / 3.
pub fn simpson(f: &impl Fn(f64) -> f64, a: f64, b: f64, n: i64) -> f64 {
    assert!(
        n > 0 && n % 2 == 0,
        "integral steps must be positive and even"
    );
    let h = (b - a) / n as f64;
    let mut acc = 0.0;
    for i in 0..=n {
        let x = a + i as f64 * h;
        let weight = if i == 0 || i == n {
            1.0
        } else if i % 2 == 0 {
            2.0
        } else {
            4.0
        };
        acc += weight * f(x);
    }
    acc * h / 3.0
}

/// Numerical limit: sample f at target ± h for geometrically decreasing h
/// (1e-1..1e-12), returning on 1% agreement between successive finite
/// samples; otherwise the last finite sample. Direction: > 0.5 approaches
/// from above, < -0.5 from below, otherwise two-sided.
pub fn sample_limit(f: &impl Fn(f64) -> f64, target: f64, direction: f64) -> f64 {
    let dirs: &[f64] = if direction > 0.5 {
        &[1.0]
    } else if direction < -0.5 {
        &[-1.0]
    } else {
        &[1.0, -1.0]
    };
    let mut best = f64::NAN;
    let mut prev = f64::NAN;
    for exp in 1u32..=12 {
        let h = 10f64.powi(-(exp as i32));
        for &dd in dirs {
            let x = target + dd * h;
            let fx = f(x);
            if fx.is_finite() {
                if prev.is_finite() && (fx - prev).abs() <= fx.abs() * 0.01 + 1e-14 {
                    return fx;
                }
                prev = fx;
                best = fx;
            }
        }
    }
    if best.is_finite() {
        best
    } else {
        panic!("sample_limit produced no finite values");
    }
}

// ── Internal helpers ──────────────────────────────────────────────────────

/// Extended GCD: returns (g, x, y) such that a*x + b*y = g = gcd(a, b).
fn extended_gcd(a: i64, b: i64) -> (i64, i64, i64) {
    if b == 0 {
        (a, 1, 0)
    } else {
        let (g, x, y) = extended_gcd(b, a.rem_euclid(b));
        (g, y, x - (a / b) * y)
    }
}

/// Row-major storage with explicit extents, including zero-row matrices.
#[derive(Clone, Debug, PartialEq)]
pub struct Matrix<T = f64> {
    rows: usize,
    cols: usize,
    data: Vec<T>,
}

impl<T> Matrix<T> {
    pub fn new(rows: usize, cols: usize, data: Vec<T>) -> Result<Self, &'static str> {
        if rows.checked_mul(cols) != Some(data.len()) {
            return Err("E-MATRIX-SHAPE: invalid dimensions or data length");
        }
        Ok(Self { rows, cols, data })
    }

    pub fn rows(&self) -> usize { self.rows }
    pub fn cols(&self) -> usize { self.cols }
    pub fn as_slice(&self) -> &[T] { &self.data }
    pub fn into_data(self) -> Vec<T> { self.data }
    pub fn get(&self, row: usize, col: usize) -> Option<&T> {
        if row >= self.rows || col >= self.cols { return None; }
        self.data.get(row * self.cols + col)
    }
}

/// Format binary64 in scientific notation without a significance policy.
/// Precision counts digits after the decimal point; allocation failure refuses.
pub fn format_scientific(value: f64, precision: i64) -> Result<String, &'static str> {
    use std::fmt::Write;
    let precision = usize::try_from(precision).map_err(|_| "E-SCALAR-CONVERT: negative precision")?;
    let capacity = if value.is_finite() {
        precision.checked_add(32).ok_or("E-SCALAR-CONVERT: precision exceeds capacity")?
    } else { 32 };
    let mut text = String::new();
    text.try_reserve_exact(capacity).map_err(|_| "E-SCALAR-CONVERT: formatting allocation failed")?;
    write!(&mut text, "{value:.precision$e}").map_err(|_| "E-SCALAR-CONVERT: formatting failed")?;
    Ok(text)
}

/// Numeric storage layout. The stored length does not certify shape validity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DenseLayout {
    Scalar,
    Vector(usize),
    Matrix { rows: usize, cols: usize, len: usize },
    Tensor { shape: Vec<usize>, len: usize },
}

impl DenseLayout {
    pub fn len(&self) -> usize {
        match self {
            Self::Scalar => 1,
            Self::Vector(len) | Self::Matrix { len, .. } | Self::Tensor { len, .. } => *len,
        }
    }
    pub fn is_empty(&self) -> bool { self.len() == 0 }
}

/// Numeric callback result carrier. Float64, Int, and vector results remain distinct.
#[derive(Clone, Debug, PartialEq)]
pub enum NumericProgramResult {
    Scalar(f64),
    Integer(i64),
    Vector(Vec<f64>),
}

impl NumericProgramResult {
    pub fn into_vector(self) -> Result<Vec<f64>, String> {
        match self {
            Self::Scalar(value) => Ok(vec![value]),
            Self::Vector(values) => Ok(values),
            Self::Integer(_) => Err("E-TYPE-012: program result must be Float64 or Vector<Float64>".into()),
        }
    }

    pub fn into_real(self) -> Result<f64, String> {
        match self {
            Self::Scalar(value) => Ok(value),
            Self::Integer(value) => Ok(value as f64),
            Self::Vector(_) => Err("E-TYPE-012: program result must be a real scalar".into()),
        }
    }

    pub fn into_scalar(self) -> Result<f64, String> {
        match self {
            Self::Scalar(value) => Ok(value),
            Self::Vector(_) | Self::Integer(_) => Err("E-TYPE-012: program result must be Float64".into()),
        }
    }
}
