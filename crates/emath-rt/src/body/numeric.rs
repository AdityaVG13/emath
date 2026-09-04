// ── Number theory / finite-field arithmetic ───────────────────────────────

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

// ── Richer linear algebra ────────────────────────────────────────
//
// Deterministic strict-f64 kernels over flat row-major storage: cyclic
// Jacobi eigen (real symmetric, ascending values, aligned unit columns),
// thin SVD via the symmetric AᵀA eigenproblem (descending singular
// values; reconstruction A = U·diag(s)·Vᵀ), and conjugate gradient
// (SPD-convergence-checked). Empty output = typed refusal upstream (the
// interpreter path surfaces E-LINALG-001..003); these kernels never
// return NaN spectra.

/// Jacobi eigenvalue decomposition of a real symmetric `rows×rows`
/// matrix (flat row-major). Returns `(values ascending, vectors
/// columns-aligned)`; empty values on non-square/non-symmetric input
/// or a convergence stall.
pub fn eig_symmetric(flat: &[f64], rows: usize, cols: usize) -> (Vec<f64>, Vec<Vec<f64>>) {
    if rows != cols || rows == 0 {
        return (Vec::new(), Vec::new());
    }
    let n = rows;
    let mut work: Vec<Vec<f64>> = (0..n)
        .map(|r| flat[r * cols..r * cols + cols].to_vec())
        .collect();
    // Symmetry gate (relative tolerance; rounding noise admits).
    let magnitude: f64 = work
        .iter()
        .flat_map(|row| row.iter())
        .map(|x| x.abs())
        .sum();
    let tolerance = 1e-9 * magnitude.max(1.0);
    for i in 0..n {
        for j in 0..n {
            if (work[i][j] - work[j][i]).abs()
                > tolerance * (work[i][j].abs() + work[j][i].abs() + 1.0)
            {
                return (Vec::new(), Vec::new());
            }
        }
    }
    let mut vectors = vec![vec![0.0; n]; n];
    for (i, row) in vectors.iter_mut().enumerate() {
        row[i] = 1.0;
    }
    let scale: f64 = work.iter().flat_map(|row| row.iter()).map(|x| x * x).sum();
    let threshold = 1e-24 * scale.max(1.0);
    for _sweep in 0..100 {
        let off: f64 = (0..n)
            .flat_map(|p| (0..n).map(move |q| (p, q)))
            .filter(|(p, q)| p != q)
            .map(|(p, q)| work[p][q] * work[p][q])
            .sum();
        if off <= threshold {
            break;
        }
        for p in 0..n {
            for q in (p + 1)..n {
                let apq = work[p][q];
                if apq.abs() <= 1e-300 {
                    continue;
                }
                let theta = (work[q][q] - work[p][p]) / (2.0 * apq);
                let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
                let c = 1.0 / (t * t + 1.0).sqrt();
                let s = c * t;
                for k in 0..n {
                    let akp = work[k][p];
                    let akq = work[k][q];
                    work[k][p] = c * akp - s * akq;
                    work[k][q] = s * akp + c * akq;
                }
                for k in 0..n {
                    let apk = work[p][k];
                    let aqk = work[q][k];
                    work[p][k] = c * apk - s * aqk;
                    work[q][k] = s * apk + c * aqk;
                }
                for k in 0..n {
                    let vkp = vectors[k][p];
                    let vkq = vectors[k][q];
                    vectors[k][p] = c * vkp - s * vkq;
                    vectors[k][q] = s * vkp + c * vkq;
                }
            }
        }
    }
    let off: f64 = (0..n)
        .flat_map(|p| (0..n).map(move |q| (p, q)))
        .filter(|(p, q)| p != q)
        .map(|(p, q)| work[p][q] * work[p][q])
        .sum();
    if off > threshold {
        return (Vec::new(), Vec::new());
    }
    // Canonical signs: the largest-|.| component of each column is +.
    for j in 0..n {
        let argmax = (0..n)
            .fold((0usize, 0.0f64), |best, i| {
                let magnitude = vectors[i][j].abs();
                if magnitude > best.1 {
                    (i, magnitude)
                } else {
                    best
                }
            })
            .0;
        if vectors[argmax][j] < 0.0 {
            for i in 0..n {
                vectors[i][j] = -vectors[i][j];
            }
        }
    }
    let mut order: Vec<usize> = (0..n).collect();
    let mut values: Vec<f64> = (0..n).map(|i| work[i][i]).collect();
    order.sort_by(|x, y| values[*x].total_cmp(&values[*y]));
    let sorted_values = order.iter().map(|i| values[*i]).collect::<Vec<_>>();
    let sorted_vectors = order
        .iter()
        .map(|i| (0..n).map(|r| vectors[r][*i]).collect::<Vec<_>>())
        .collect();
    values = sorted_values;
    (values, sorted_vectors)
}

/// Eigenvalues only (ascending); empty on refusal.
pub fn eig_values_flat(flat: &[f64], rows: usize, cols: usize) -> Vec<f64> {
    eig_symmetric(flat, rows, cols).0
}

/// Eigenvector matrix (flat row-major, column j for eigenvalue j);
/// empty on refusal.
pub fn eig_vectors_flat(flat: &[f64], rows: usize, cols: usize) -> Vec<f64> {
    let (values, vectors) = eig_symmetric(flat, rows, cols);
    if values.is_empty() {
        return Vec::new();
    }
    let n = rows;
    let mut out = vec![0.0; n * n];
    for (j, column) in vectors.iter().enumerate() {
        for (i, entry) in column.iter().enumerate() {
            out[i * n + j] = *entry;
        }
    }
    out
}

/// Singular values of a rectangular matrix, DESCENDING (thin rank via
/// the symmetric AᵀA eigenproblem); empty on refusal.
pub fn svd_values_flat(flat: &[f64], rows: usize, cols: usize) -> Vec<f64> {
    let (singular, _) = svd_thin_flat(flat, rows, cols);
    singular
}

/// Packed `[U; s; Vᵀ]` thin-SVD factors (width max(cols, rank), zero
/// padding; see the EMIR op docs); empty on refusal.
pub fn svd_factors_flat(flat: &[f64], rows: usize, cols: usize) -> Vec<f64> {
    let (singular, factors) = svd_thin_flat(flat, rows, cols);
    if singular.is_empty() {
        return Vec::new();
    }
    factors
}

/// Thin SVD core: returns `(s descending, packed [U; s; Vᵀ])`.
fn svd_thin_flat(flat: &[f64], rows: usize, cols: usize) -> (Vec<f64>, Vec<f64>) {
    if rows == 0 || cols == 0 || flat.len() != rows * cols {
        return (Vec::new(), Vec::new());
    }
    if flat.iter().any(|x| !x.is_finite()) {
        return (Vec::new(), Vec::new());
    }
    // AᵀA (cols×cols, symmetric PSD).
    let mut ata = vec![vec![0.0; cols]; cols];
    for i in 0..cols {
        for j in 0..cols {
            ata[i][j] = (0..rows)
                .map(|k| flat[k * cols + i] * flat[k * cols + j])
                .sum();
        }
    }
    let (eigenvalues, vectors) = eig_symmetric(
        &ata.iter().flatten().copied().collect::<Vec<f64>>(),
        cols,
        cols,
    );
    if eigenvalues.is_empty() {
        return (Vec::new(), Vec::new());
    }
    let rank = rows.min(cols);
    // Descending order; keep the thin rank.
    let mut order: Vec<usize> = (0..cols).collect();
    order.sort_by(|x, y| eigenvalues[*y].total_cmp(&eigenvalues[*x]));
    order.truncate(rank);
    let singular: Vec<f64> = order
        .iter()
        .map(|i| eigenvalues[*i].max(0.0).sqrt())
        .collect();
    // V rows (columns of V, i.e. rows of Vᵀ) in descending order.
    let v_rows: Vec<Vec<f64>> = order
        .iter()
        .map(|source| (0..cols).map(|row| vectors[row][*source]).collect())
        .collect();
    // U columns: u_k = A·v_k / σ_k (zero column for σ ≈ 0).
    let width = cols.max(rank);
    let out_rows = rows + 1 + rank;
    let mut packed = vec![0.0; out_rows * width];
    for (k, sigma) in singular.iter().enumerate() {
        if *sigma <= 1e-12 {
            continue; // rank-deficient direction: zero column (documented)
        }
        for row in 0..rows {
            let dot: f64 = (0..cols).map(|i| flat[row * cols + i] * v_rows[k][i]).sum();
            packed[row * width + k] = dot / sigma;
        }
        // Vᵀ row k.
        let base = (rows + 1 + k) * width;
        packed[base..base + cols].copy_from_slice(&v_rows[k]);
    }
    // s row.
    packed[rows * width..rows * width + rank].copy_from_slice(&singular);
    (singular, packed)
}

/// Conjugate gradient over flat row-major dense storage: solves
/// `A x = b` for SPD `A` (200 iterations, 1e-10 relative tolerance).
/// Empty result = non-convergence (typed upstream, never a wrong x).
pub fn cg_solve_flat(a_flat: &[f64], rows: usize, cols: usize, b: &[f64]) -> Vec<f64> {
    if rows != cols || rows == 0 || b.len() != rows || a_flat.len() != rows * cols {
        return Vec::new();
    }
    let n = rows;
    let mat_vec = |x: &[f64]| -> Vec<f64> {
        (0..n)
            .map(|i| (0..n).map(|j| a_flat[i * n + j] * x[j]).sum())
            .collect()
    };
    let b_norm: f64 = b.iter().map(|x| x * x).sum::<f64>().sqrt().max(1e-300);
    let mut x = vec![0.0; n];
    let mut residual = b.to_vec();
    let mut direction = residual.clone();
    let mut residual_norm_sq: f64 = residual.iter().map(|x| x * x).sum();
    for _iteration in 0..200 {
        if residual_norm_sq.sqrt() <= 1e-10 * b_norm {
            return x;
        }
        let adirection = mat_vec(&direction);
        let denominator: f64 = direction
            .iter()
            .zip(adirection.iter())
            .map(|(d, ad)| d * ad)
            .sum();
        if denominator <= 0.0 || !denominator.is_finite() {
            return Vec::new(); // non-SPD step: typed refusal upstream
        }
        let step = residual_norm_sq / denominator;
        for (x_i, d_i) in x.iter_mut().zip(direction.iter()) {
            *x_i += step * d_i;
        }
        for (r_i, ad_i) in residual.iter_mut().zip(adirection.iter()) {
            *r_i -= step * ad_i;
        }
        let new_norm_sq: f64 = residual.iter().map(|x| x * x).sum();
        let beta = new_norm_sq / residual_norm_sq;
        for (d_i, r_i) in direction.iter_mut().zip(residual.iter()) {
            *d_i = *r_i + beta * *d_i;
        }
        residual_norm_sq = new_norm_sq;
    }
    if residual_norm_sq.sqrt() <= 1e-10 * b_norm {
        return x;
    }
    Vec::new()
}

/// Dense partial-pivot solve of `A x = b`; empty on a singular,
/// non-finite, or shape-invalid system.
pub fn linear_solve_flat(a_flat: &[f64], rows: usize, cols: usize, b: &[f64]) -> Vec<f64> {
    if rows == 0
        || rows != cols
        || a_flat.len() != rows * cols
        || b.len() != rows
        || a_flat.iter().chain(b).any(|value| !value.is_finite())
    {
        return Vec::new();
    }
    let n = rows;
    let mut a = a_flat.to_vec();
    let mut rhs = b.to_vec();
    for column in 0..n {
        let pivot = (column..n)
            .max_by(|left, right| {
                a[*left * n + column]
                    .abs()
                    .total_cmp(&a[*right * n + column].abs())
            })
            .unwrap_or(column);
        if a[pivot * n + column].abs() <= 1e-14 {
            return Vec::new();
        }
        if pivot != column {
            for j in 0..n {
                a.swap(column * n + j, pivot * n + j);
            }
            rhs.swap(column, pivot);
        }
        for row in (column + 1)..n {
            let factor = a[row * n + column] / a[column * n + column];
            a[row * n + column] = 0.0;
            for j in (column + 1)..n {
                a[row * n + j] -= factor * a[column * n + j];
            }
            rhs[row] -= factor * rhs[column];
        }
    }
    let mut solution = vec![0.0; n];
    for row in (0..n).rev() {
        let residual = rhs[row]
            - ((row + 1)..n)
                .map(|column| a[row * n + column] * solution[column])
                .sum::<f64>();
        solution[row] = residual / a[row * n + row];
    }
    solution
}

/// Packed partial-pivot LU factorization `[p; L; U]`, with permutation
/// row `p` followed by `n` rows of `L` and `n` rows of `U`.
pub fn lu_factors_flat(a_flat: &[f64], rows: usize, cols: usize) -> Vec<f64> {
    if rows == 0
        || rows != cols
        || a_flat.len() != rows * cols
        || a_flat.iter().any(|value| !value.is_finite())
    {
        return Vec::new();
    }
    let n = rows;
    let mut lu = a_flat.to_vec();
    let mut permutation: Vec<usize> = (0..n).collect();
    for column in 0..n {
        let pivot = (column..n)
            .max_by(|left, right| {
                lu[*left * n + column]
                    .abs()
                    .total_cmp(&lu[*right * n + column].abs())
            })
            .unwrap_or(column);
        if lu[pivot * n + column].abs() <= 1e-14 {
            return Vec::new();
        }
        if pivot != column {
            for j in 0..n {
                lu.swap(column * n + j, pivot * n + j);
            }
            permutation.swap(column, pivot);
        }
        for row in (column + 1)..n {
            lu[row * n + column] /= lu[column * n + column];
            for j in (column + 1)..n {
                lu[row * n + j] -= lu[row * n + column] * lu[column * n + j];
            }
        }
    }
    let mut packed = vec![0.0; (2 * n + 1) * n];
    for (column, source) in permutation.into_iter().enumerate() {
        packed[column] = source as f64;
    }
    for row in 0..n {
        for column in 0..n {
            packed[(row + 1) * n + column] = if row == column {
                1.0
            } else if row > column {
                lu[row * n + column]
            } else {
                0.0
            };
            packed[(n + 1 + row) * n + column] = if row <= column {
                lu[row * n + column]
            } else {
                0.0
            };
        }
    }
    packed
}

/// Packed thin QR factorization `[Q; R]` for `m >= n`, with `m` rows
/// of `Q` followed by `n` rows of `R`; empty for rank deficiency.
pub fn qr_factors_flat(a_flat: &[f64], rows: usize, cols: usize) -> Vec<f64> {
    if rows == 0
        || cols == 0
        || rows < cols
        || a_flat.len() != rows * cols
        || a_flat.iter().any(|value| !value.is_finite())
    {
        return Vec::new();
    }
    let mut q = vec![0.0; rows * cols];
    let mut r = vec![0.0; cols * cols];
    for column in 0..cols {
        let mut vector = (0..rows)
            .map(|row| a_flat[row * cols + column])
            .collect::<Vec<_>>();
        for previous in 0..column {
            let projection = (0..rows)
                .map(|row| q[row * cols + previous] * vector[row])
                .sum::<f64>();
            r[previous * cols + column] = projection;
            for row in 0..rows {
                vector[row] -= projection * q[row * cols + previous];
            }
        }
        let norm = vector.iter().map(|value| value * value).sum::<f64>().sqrt();
        if norm <= 1e-14 {
            return Vec::new();
        }
        r[column * cols + column] = norm;
        for row in 0..rows {
            q[row * cols + column] = vector[row] / norm;
        }
    }
    q.extend(r);
    q
}

