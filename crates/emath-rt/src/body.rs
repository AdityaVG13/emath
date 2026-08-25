//! Pre-compiled math kernels for emath-generated crates and the emath
//! interpreter.
//!
//! This file is the single source of truth for the numeric algorithms that
//! generated Rust would otherwise inline per-use: vector/matrix/tensor
//! arithmetic, stencil convolution, modular arithmetic, and the
//! higher-order drivers (fold, Simpson quadrature, numerical limits).
//!
//! It is embedded verbatim into every generated crate as
//! `mod emath_rt { ... }` (via the `SOURCE` constant in `lib.rs`), so
//! generated artifacts stay self-contained with no external dependencies.
//! Two rules keep that embedding sound:
//!
//! 1. std-only: no external crates, no `crate::` paths, no crate-level
//!    attributes (the text is pasted inside an existing module block).
//! 2. Deterministic: same inputs, same IEEE-754 operation order, same
//!    output, bit-for-bit.

// ── Vectors ────────────────────────────────────────────────────────────────

/// Elementwise add of two vectors (zip semantics: extra elements past the
/// shorter operand are dropped, matching `zip`).
pub fn vec_add(a: &[f64], b: &[f64]) -> Vec<f64> {
    a.iter().zip(b.iter()).map(|(x, y)| x + y).collect()
}

/// Elementwise subtract of two vectors (zip semantics).
pub fn vec_sub(a: &[f64], b: &[f64]) -> Vec<f64> {
    a.iter().zip(b.iter()).map(|(x, y)| x - y).collect()
}

/// Scale a vector by a scalar.
pub fn vec_scale(v: &[f64], s: f64) -> Vec<f64> {
    v.iter().map(|x| x * s).collect()
}

/// Dot product of two vectors (zip semantics).
pub fn vec_dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

/// Euclidean (L2) norm of a vector.
pub fn vec_norm(v: &[f64]) -> f64 {
    v.iter().map(|x| x * x).sum::<f64>().sqrt()
}

// ── Matrices (row-major nested rows) ──────────────────────────────────────

/// Elementwise add of two matrices (zip semantics at both levels).
pub fn mat_add(a: &[Vec<f64>], b: &[Vec<f64>]) -> Vec<Vec<f64>> {
    a.iter()
        .zip(b.iter())
        .map(|(r1, r2)| r1.iter().zip(r2.iter()).map(|(x, y)| x + y).collect())
        .collect()
}

/// Elementwise subtract of two matrices (zip semantics at both levels).
pub fn mat_sub(a: &[Vec<f64>], b: &[Vec<f64>]) -> Vec<Vec<f64>> {
    a.iter()
        .zip(b.iter())
        .map(|(r1, r2)| r1.iter().zip(r2.iter()).map(|(x, y)| x - y).collect())
        .collect()
}

/// Scale a matrix by a scalar.
pub fn mat_scale(m: &[Vec<f64>], s: f64) -> Vec<Vec<f64>> {
    m.iter().map(|row| row.iter().map(|x| x * s).collect()).collect()
}

/// Matrix times vector: result[r] = sum_c m[r][c] * v[c] (zip semantics
/// per row: extra vector entries past the row length are dropped).
pub fn mat_mul_vec(m: &[Vec<f64>], v: &[f64]) -> Vec<f64> {
    m.iter()
        .map(|row| row.iter().zip(v.iter()).map(|(a, b)| a * b).sum())
        .collect()
}

/// Matrix product. Mirrors the historical inline semantics exactly:
/// dimensions are read from the operands (empty first dimension treated
/// as 0), and inner-dimension indexing is direct (ragged operands panic,
/// as the inline generated code did).
pub fn mat_mul_mat(a: &[Vec<f64>], b: &[Vec<f64>]) -> Vec<Vec<f64>> {
    let r1 = a.len();
    let c2 = if b.is_empty() { 0 } else { b[0].len() };
    let c1 = if a.is_empty() { 0 } else { a[0].len() };
    (0..r1)
        .map(|i| (0..c2).map(|j| (0..c1).map(|k| a[i][k] * b[k][j]).sum::<f64>()).collect())
        .collect()
}

/// Transpose a matrix (empty matrix stays empty).
pub fn mat_transpose(m: &[Vec<f64>]) -> Vec<Vec<f64>> {
    if m.is_empty() {
        return vec![];
    }
    let rows = m.len();
    let cols = m[0].len();
    (0..cols)
        .map(|c| (0..rows).map(|r| m[r][c]).collect())
        .collect()
}

// ── Tensors (flat storage) ────────────────────────────────────────────────

/// Elementwise add of two flat tensors (zip semantics).
pub fn tensor_add(a: &[f64], b: &[f64]) -> Vec<f64> {
    a.iter().zip(b.iter()).map(|(x, y)| x + y).collect()
}

/// Elementwise subtract of two flat tensors (zip semantics).
pub fn tensor_sub(a: &[f64], b: &[f64]) -> Vec<f64> {
    a.iter().zip(b.iter()).map(|(x, y)| x - y).collect()
}

// ── Stencils ──────────────────────────────────────────────────────────────

/// Edge policy for stencil convolution: how out-of-range taps are resolved.
pub enum EdgePolicy {
    /// Replicate the nearest in-range cell (zero-gradient / insulated).
    Clamp,
    /// Mirror across the boundary: u[-1] = u[1], u[n] = u[n-2]
    /// (second-order zero-gradient; trailing clamp guards tiny inputs).
    Neumann,
    /// Hold the boundary at fixed values.
    Dirichlet { left: f64, right: f64 },
}

/// 1D stencil convolution. `center` is the tap index that maps to the
/// output cell. Mirrors the historical inline semantics, including the
/// exact boundary math per edge policy.
pub fn stencil_1d(input: &[f64], weights: &[f64], center: i64, edge: &EdgePolicy) -> Vec<f64> {
    let n = input.len();
    let last = n.saturating_sub(1) as isize;
    (0..n)
        .map(|i| {
            weights
                .iter()
                .enumerate()
                .map(|(k, &w)| {
                    let raw = i as isize + k as isize - center as isize;
                    match edge {
                        EdgePolicy::Clamp => w * input[raw.clamp(0, last) as usize],
                        EdgePolicy::Neumann => {
                            let idx = if raw < 0 {
                                (-raw) as usize
                            } else if raw > last {
                                (2 * last - raw) as usize
                            } else {
                                raw as usize
                            };
                            w * input[idx.clamp(0, last as usize)]
                        }
                        EdgePolicy::Dirichlet { left, right } => {
                            if raw < 0 {
                                w * left
                            } else if raw > last {
                                w * right
                            } else {
                                w * input[raw as usize]
                            }
                        }
                    }
                })
                .sum()
        })
        .collect()
}

/// 2D 3x3 stencil convolution. `center` is the (row, col) tap offset of
/// the center weight. Dirichlet is refused (mirrors the backend, which
/// refuses 2D Dirichlet at codegen time; the interpreter pre-checks it).
pub fn stencil_2d(
    input: &[Vec<f64>],
    weights: &[f64; 9],
    center: (i64, i64),
    edge: &EdgePolicy,
) -> Vec<Vec<f64>> {
    let nr = input.len();
    let nc = if nr == 0 { 0 } else { input[0].len() };
    let lr = nr.saturating_sub(1) as isize;
    let lc = nc.saturating_sub(1) as isize;
    let (cr, cc) = center;
    let mut out = Vec::with_capacity(nr);
    for r in 0..nr {
        let mut row = Vec::with_capacity(nc);
        for c in 0..nc {
            let mut acc = 0.0f64;
            for kr in 0..3usize {
                for kc in 0..3usize {
                    let w = weights[kr * 3 + kc];
                    let raw_r = r as isize + kr as isize - cr as isize;
                    let raw_c = c as isize + kc as isize - cc as isize;
                    acc += match edge {
                        EdgePolicy::Clamp => {
                            let rr = raw_r.clamp(0, lr) as usize;
                            let cc2 = raw_c.clamp(0, lc) as usize;
                            w * input[rr][cc2]
                        }
                        EdgePolicy::Neumann => {
                            let rr = (if raw_r < 0 {
                                -raw_r
                            } else if raw_r > lr {
                                2 * lr - raw_r
                            } else {
                                raw_r
                            })
                            .clamp(0, lr) as usize;
                            let cc2 = (if raw_c < 0 {
                                -raw_c
                            } else if raw_c > lc {
                                2 * lc - raw_c
                            } else {
                                raw_c
                            })
                            .clamp(0, lc) as usize;
                            w * input[rr][cc2]
                        }
                        EdgePolicy::Dirichlet { .. } => {
                            panic!("stencil_2d: Dirichlet boundary is not supported")
                        }
                    };
                }
            }
            row.push(acc);
        }
        out.push(row);
    }
    out
}

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
    let mut result: i64 = 0;
    for &c in coeffs.iter().rev() {
        result = (result * x + c as i64).rem_euclid(p);
    }
    Ok(result)
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
        let mut result: i64 = 0;
        for &c in coeffs.iter().rev() {
            result = (result * x + c as i64).rem_euclid(p);
        }
        codeword.push(result as f64);
    }
    Ok(codeword)
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

/// Composite Simpson's rule quadrature over an even positive panel count
/// (panics otherwise). Mirrors the historical inline order: h = (b-a)/n,
/// weights 1/4/2.../4/1, acc * h / 3.
pub fn simpson(f: &impl Fn(f64) -> f64, a: f64, b: f64, n: i64) -> f64 {
    assert!(n > 0 && n % 2 == 0, "integral steps must be positive and even");
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
