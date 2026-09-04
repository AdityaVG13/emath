// Pre-compiled math kernels, embedded verbatim into generated crates as
// `mod emath_rt { ... }`. Keep this file std-only (no external crates, no
// `crate::` paths, no crate attributes) and deterministic: same inputs,
// same IEEE-754 operation order, bit-for-bit same output.

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

/// Euclidean (L2) norm of a vector. Empty input is +0.0 (empty sum of
/// squares); `Iterator::sum` on `f64` starts at `-0.0`, and `sqrt(-0.0)`
/// would leak a negative zero that is not a length.
pub fn vec_norm(v: &[f64]) -> f64 {
    v.iter().fold(0.0, |acc, x| acc + x * x).sqrt()
}

/// Principal square root of `re + im i`. Negative reals with `im = +0`
/// map to `+i` (`sqrt(-1) = i`).
pub fn complex_sqrt(re: f64, im: f64) -> (f64, f64) {
    if re == 0.0 && im == 0.0 {
        return (0.0, 0.0);
    }
    let r = re.hypot(im);
    let sr = ((r + re) * 0.5).sqrt();
    let si = ((r - re) * 0.5).sqrt();
    (sr, im.signum() * si)
}

/// Principal logarithm `ln|z| + i Arg(z)` with `Arg = atan2(im, re)`.
pub fn complex_ln(re: f64, im: f64) -> (f64, f64) {
    (re.hypot(im).ln(), im.atan2(re))
}

/// `exp(re + im i) = e^{re} (cos(im) + i sin(im))`.
pub fn complex_exp(re: f64, im: f64) -> (f64, f64) {
    let scale = re.exp();
    (scale * im.cos(), scale * im.sin())
}

/// Trapezoidal (half-cell at each end) sum: conserves Neumann-mirror heat.
pub fn trapezoid_sum(u: &[f64]) -> f64 {
    match u.len() {
        0 => 0.0,
        1 => u[0],
        n => {
            let mid: f64 = u[1..n - 1].iter().copied().sum();
            0.5 * u[0] + mid + 0.5 * u[n - 1]
        }
    }
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
    m.iter()
        .map(|row| row.iter().map(|x| x * s).collect())
        .collect()
}

/// Matrix times vector: result[r] = sum_c m[r][c] * v[c] (zip semantics
/// per row).
pub fn mat_mul_vec(m: &[Vec<f64>], v: &[f64]) -> Vec<f64> {
    m.iter()
        .map(|row| row.iter().zip(v.iter()).map(|(a, b)| a * b).sum())
        .collect()
}

/// Primitive INTEGER null vector of an integer matrix, `Ok(None)` when
/// the nullspace is not exactly one-dimensional. Exact rational
/// Gauss-Jordan elimination with i128 intermediates and gcd-reduced
/// fractions (no floating point anywhere); the returned vector is
/// primitive (entries coprime, first nonzero entry positive). This is
/// the generic exact-integer primitive the balancing cell composes;
/// it contains no domain logic. Ragged rows and i128 overflow are
/// caller-visible errors, never silent truncation.
pub fn primitive_int_nullvector(rows: &[Vec<i64>]) -> Result<Option<Vec<i64>>, String> {
    if rows.is_empty() {
        return Err("empty matrix has no nullspace".to_string());
    }
    let n = rows[0].len();
    if n == 0 {
        return Err("zero-column matrix has no nullspace".to_string());
    }
    for row in rows {
        if row.len() != n {
            return Err("ragged rows have no common width".to_string());
        }
    }
    // Rational matrix (num, den) with den > 0, gcd-reduced at each step.
    let mut a: Vec<Vec<(i128, i128)>> = rows
        .iter()
        .map(|row| row.iter().map(|&x| (i128::from(x), 1)).collect())
        .collect();
    let m = a.len();
    let mut pivot_cols: Vec<usize> = Vec::new();
    let mut cur = 0usize;
    for col in 0..n {
        let Some(pivot) = (cur..m).find(|&r| a[r][col].0 != 0) else {
            continue;
        };
        a.swap(cur, pivot);
        // Normalize the pivot row so the pivot entry is (1, 1).
        let (pnum, pden) = a[cur][col];
        for entry in &mut a[cur] {
            let (num, den) = *entry;
            let num = num.checked_mul(pden).ok_or_else(overflow)?;
            let den = den.checked_mul(pnum).ok_or_else(overflow)?;
            *entry = reduce(num, den)?;
        }
        // Eliminate the pivot column in every OTHER row.
        for r in 0..m {
            if r == cur {
                continue;
            }
            let (fnum, fden) = a[r][col];
            if fnum == 0 {
                continue;
            }
            for c in 0..n {
                let (pnum, pden) = a[cur][c];
                let (rnum, rden) = a[r][c];
                // r[c] - (fnum/fden) * pivot[c]:
                //   (rnum/rden) - (fnum*fden_inv)*(pnum/pden)
                let left_num = rnum.checked_mul(fden).ok_or_else(overflow)?;
                let left_den = rden.checked_mul(fden).ok_or_else(overflow)?;
                let right_num = fnum
                    .checked_mul(pnum)
                    .ok_or_else(overflow)?
                    .checked_mul(rden)
                    .ok_or_else(overflow)?;
                let right_den = fden
                    .checked_mul(pden)
                    .ok_or_else(overflow)?
                    .checked_mul(rden)
                    .ok_or_else(overflow)?;
                let left = (left_num, left_den);
                let right = (right_num, right_den);
                let num = left
                    .0
                    .checked_mul(right.1)
                    .ok_or_else(overflow)?
                    .checked_sub(right.0.checked_mul(left.1).ok_or_else(overflow)?)
                    .ok_or_else(overflow)?;
                let den = left.1.checked_mul(right.1).ok_or_else(overflow)?;
                a[r][c] = reduce(num, den)?;
            }
        }
        pivot_cols.push(col);
        cur += 1;
    }
    let dim = n - pivot_cols.len();
    if dim != 1 {
        return Ok(None);
    }
    // The single free column.
    let free = (0..n)
        .find(|c| !pivot_cols.contains(c))
        .expect("dimension 1 has one free column");
    // Back-substitute with x[free] = 1.
    let mut x: Vec<(i128, i128)> = vec![(0, 1); n];
    x[free] = (1, 1);
    for (i, &pc) in pivot_cols.iter().enumerate() {
        // Pivot row is normalized: x[pc] + row[free] * x[free] = 0.
        let (fnum, fden) = a[i][free];
        x[pc] = reduce(fnum.checked_neg().ok_or_else(overflow)?, fden)?;
    }
    // Scale by the LCM of positive denominators -> integer vector.
    let mut scale = 1i128;
    for (num, den) in &x {
        if *num != 0 {
            scale = lcm_128(scale, *den)?;
        }
    }
    let mut out: Vec<i64> = Vec::with_capacity(n);
    let mut g = 0i128;
    for (num, den) in x {
        let int = num.checked_mul(scale).ok_or_else(overflow)? / den;
        if int != 0 {
            g = gcd_128(g, int.abs());
        }
        out.push(i64::try_from(int).map_err(|_| overflow())?);
    }
    if g > 1 {
        let g = i64::try_from(g).map_err(|_| overflow())?;
        for v in &mut out {
            *v /= g;
        }
    }
    // Canonical sign: first nonzero entry positive.
    if out.iter().find(|&&v| v != 0).is_some_and(|&v| v < 0) {
        for v in &mut out {
            *v = v.checked_neg().ok_or_else(overflow)?;
        }
    }
    Ok(Some(out))
}

/// gcd-reduced rational; denominator forced positive.
fn reduce(num: i128, den: i128) -> Result<(i128, i128), String> {
    if den == 0 {
        return Err("zero denominator".to_string());
    }
    let (num, den) = if den < 0 { (-num, -den) } else { (num, den) };
    let mut g = gcd_128(num.abs(), den);
    if g == 0 {
        g = 1;
    }
    let num = num.checked_div(g).ok_or_else(overflow)?;
    let den = den.checked_div(g).ok_or_else(overflow)?;
    Ok((num, den))
}

/// Absolute-value gcd over i128.
fn gcd_128(mut a: i128, mut b: i128) -> i128 {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a.abs()
}

/// Least common multiple over i128 (0 when either side is 0).
fn lcm_128(a: i128, b: i128) -> Result<i128, String> {
    if a == 0 || b == 0 {
        return Ok(0);
    }
    let g = gcd_128(a, b);
    a.checked_mul(b / g).ok_or_else(overflow)
}

/// Shared overflow message for the exact-integer path.
fn overflow() -> String {
    "exact-integer overflow in nullspace elimination".to_string()
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
        .map(|i| {
            (0..c2)
                .map(|j| (0..c1).map(|k| a[i][k] * b[k][j]).sum::<f64>())
                .collect()
        })
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

/// Scale a flat tensor by a scalar.
pub fn tensor_scale(v: &[f64], s: f64) -> Vec<f64> {
    v.iter().map(|x| x * s).collect()
}

/// Rank-3+ tensor with explicit shape. Generated crates cannot recover
/// rank from a bare `Vec<f64>` (length 8 is `[8]`, `[2,4]`, or `[2,2,2]`).
#[derive(Clone, Debug, PartialEq)]
pub struct Tensor {
    pub shape: Vec<usize>,
    pub data: Vec<f64>,
}

/// Typed index/slice refusal. The interpreter maps this to `EvalFault`;
/// generated crates `?` it into `Result<_, String>` (never panicking `[]`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IndexError {
    /// Index was not a finite whole number in `0..len` (or `0..=len` for
    /// a slice endpoint). Negative indices are refused, not wrapped.
    OutOfBounds { index: i64, len: usize },
    /// Rank/offset precondition failed.
    Arithmetic(&'static str),
}

impl std::fmt::Display for IndexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Self::OutOfBounds { index, len } => {
                write!(f, "index {index} is outside 0..{len}")
            }
            Self::Arithmetic(detail) => f.write_str(detail),
        }
    }
}

/// One axis of a tensor slice: a scalar point (drops rank) or a
/// half-open range (keeps rank).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SliceAxis {
    Point(f64),
    Range { start: f64, end: f64 },
}

/// Finite whole `raw` in `0..len`. Negative, NaN, Inf, and fractional
/// values are `OutOfBounds` — never `as usize` wrap/saturate.
pub fn whole_index(raw: f64, len: usize) -> Result<usize, IndexError> {
    if !raw.is_finite() || raw < 0.0 || raw.fract() != 0.0 {
        return Err(IndexError::OutOfBounds {
            index: raw as i64,
            len,
        });
    }
    let index = raw as usize;
    if index >= len {
        return Err(IndexError::OutOfBounds {
            index: i64::try_from(index).unwrap_or(i64::MAX),
            len,
        });
    }
    Ok(index)
}

/// Vector `v[i]` with the same bounds as the interpreter.
pub fn vec_index_checked(v: &[f64], index: f64) -> Result<f64, IndexError> {
    let i = whole_index(index, v.len())?;
    Ok(v[i])
}

/// Nested-row matrix `m[r][c]` with per-axis bounds (ragged rows use
/// that row's length).
pub fn mat_index_checked(m: &[Vec<f64>], row: f64, col: f64) -> Result<f64, IndexError> {
    let r = whole_index(row, m.len())?;
    let c = whole_index(col, m[r].len())?;
    Ok(m[r][c])
}

/// Row-major tensor `t[i, j, …]` with one index per axis.
pub fn tensor_index_checked(
    shape: &[usize],
    data: &[f64],
    indices: &[f64],
) -> Result<f64, IndexError> {
    if indices.len() != shape.len() {
        return Err(IndexError::Arithmetic(
            "tensor index rank does not match shape",
        ));
    }
    let expected = shape_product(shape).ok_or(IndexError::Arithmetic("tensor size overflow"))?;
    if data.len() != expected {
        return Err(IndexError::Arithmetic(
            "tensor data length does not match shape product",
        ));
    }
    let mut offset = 0usize;
    for (axis, &raw) in indices.iter().enumerate() {
        let i = whole_index(raw, shape[axis])?;
        offset = offset
            .checked_mul(shape[axis])
            .and_then(|base| base.checked_add(i))
            .ok_or(IndexError::Arithmetic("tensor index offset overflow"))?;
    }
    data.get(offset).copied().ok_or(IndexError::OutOfBounds {
        index: i64::try_from(offset).unwrap_or(i64::MAX),
        len: data.len(),
    })
}

/// Slice `t[0, :, :]` / `v[1:3]`: point axes drop rank, range axes keep
/// it. Result is `(kept_shape, row-major data)`.
pub fn tensor_slice_checked(
    shape: &[usize],
    data: &[f64],
    axes: &[SliceAxis],
) -> Result<(Vec<usize>, Vec<f64>), IndexError> {
    if axes.len() != shape.len() {
        return Err(IndexError::Arithmetic(
            "tensor slice rank does not match shape",
        ));
    }
    let expected = shape_product(shape).ok_or(IndexError::Arithmetic("tensor size overflow"))?;
    if data.len() != expected {
        return Err(IndexError::Arithmetic(
            "tensor/matrix data length does not match shape",
        ));
    }
    let mut starts = Vec::with_capacity(axes.len());
    let mut out_shape = Vec::with_capacity(axes.len());
    for (axis, slice) in axes.iter().enumerate() {
        match *slice {
            SliceAxis::Point(index) => {
                let i = whole_index(index, shape[axis])?;
                starts.push(i);
                out_shape.push(1);
            }
            SliceAxis::Range { start, end } => {
                let start_i = whole_index(start, shape[axis].saturating_add(1))?;
                if !end.is_finite() || end < 0.0 || end.fract() != 0.0 {
                    return Err(IndexError::OutOfBounds {
                        index: end as i64,
                        len: shape[axis],
                    });
                }
                let end_i = end as usize;
                if end_i > shape[axis] || start_i > end_i {
                    return Err(IndexError::OutOfBounds {
                        index: i64::try_from(end_i).unwrap_or(i64::MAX),
                        len: shape[axis],
                    });
                }
                starts.push(start_i);
                out_shape.push(end_i - start_i);
            }
        }
    }
    let mut out = Vec::new();
    collect_slice(data, shape, &starts, &out_shape, 0, 0, &mut out)?;
    let kept: Vec<usize> = axes
        .iter()
        .zip(out_shape)
        .filter_map(|(axis, extent)| matches!(axis, SliceAxis::Range { .. }).then_some(extent))
        .collect();
    Ok((kept, out))
}

fn shape_product(shape: &[usize]) -> Option<usize> {
    shape
        .iter()
        .try_fold(1usize, |acc, &dim| acc.checked_mul(dim))
}

fn collect_slice(
    data: &[f64],
    shape: &[usize],
    starts: &[usize],
    out_shape: &[usize],
    axis: usize,
    offset: usize,
    out: &mut Vec<f64>,
) -> Result<(), IndexError> {
    if axis == shape.len() {
        let value = data.get(offset).copied().ok_or(IndexError::OutOfBounds {
            index: i64::try_from(offset).unwrap_or(i64::MAX),
            len: data.len(),
        })?;
        out.push(value);
        return Ok(());
    }
    let stride = shape[axis + 1..]
        .iter()
        .try_fold(1usize, |acc, &d| acc.checked_mul(d))
        .ok_or(IndexError::Arithmetic("tensor slice offset overflow"))?
        .max(1);
    for i in 0..out_shape[axis] {
        let next = offset
            .checked_add(
                starts[axis]
                    .checked_add(i)
                    .and_then(|idx| idx.checked_mul(stride))
                    .ok_or(IndexError::Arithmetic("tensor slice offset overflow"))?,
            )
            .ok_or(IndexError::Arithmetic("tensor slice offset overflow"))?;
        collect_slice(data, shape, starts, out_shape, axis + 1, next, out)?;
    }
    Ok(())
}

/// Rank-0 slice result (`t[i, j, k]`-via-points, or empty kept axes).
pub fn tensor_slice_as_scalar(
    shape: &[usize],
    data: &[f64],
    axes: &[SliceAxis],
) -> Result<f64, IndexError> {
    let (_, out) = tensor_slice_checked(shape, data, axes)?;
    Ok(out.first().copied().unwrap_or(f64::NAN))
}

/// Rank-1 slice result (`v[1:3]`, `t[0, :, 0]`).
pub fn tensor_slice_as_vector(
    shape: &[usize],
    data: &[f64],
    axes: &[SliceAxis],
) -> Result<Vec<f64>, IndexError> {
    tensor_slice_checked(shape, data, axes).map(|out| out.1)
}

/// Rank-2 slice result as nested rows (`t[0, :, :]`).
pub fn tensor_slice_as_matrix(
    shape: &[usize],
    data: &[f64],
    axes: &[SliceAxis],
) -> Result<Vec<Vec<f64>>, IndexError> {
    tensor_slice_checked(shape, data, axes).map(einsum_matrix)
}

/// Rank-3+ slice result (shape + flat row-major data).
pub fn tensor_slice_as_tensor(
    shape: &[usize],
    data: &[f64],
    axes: &[SliceAxis],
) -> Result<Tensor, IndexError> {
    let (kept, data) = tensor_slice_checked(shape, data, axes)?;
    Ok(Tensor { shape: kept, data })
}

