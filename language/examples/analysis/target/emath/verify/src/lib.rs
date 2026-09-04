#![forbid(unsafe_code)]
#![allow(dead_code)]
#[allow(dead_code)]
pub mod emath_rt {
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

// ── Einstein summation ────────────────────────────────────────────────────

/// Typed einsum refusal. The interpreter maps this to `EvalFault`;
/// generated crates call the panicking `einsum_as_*` wrappers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EinsumError {
    /// Subscript / extent precondition failed.
    Arithmetic(&'static str),
    /// A contracted or output index fell outside an operand axis.
    IndexOutOfBounds { index: i64, len: usize },
}

impl std::fmt::Display for EinsumError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Self::Arithmetic(detail) => f.write_str(detail),
            Self::IndexOutOfBounds { index, len } => {
                write!(f, "einsum index {index} is outside 0..{len}")
            }
        }
    }
}

/// Flatten a vector or nested matrix into `(shape, row-major data)` so
/// generated code can pass mixed operands without the backend knowing
/// the Rust type of each register.
pub trait EinsumIn {
    fn einsum_operand(&self) -> (Vec<usize>, Vec<f64>);
}

impl EinsumIn for Vec<f64> {
    fn einsum_operand(&self) -> (Vec<usize>, Vec<f64>) {
        (vec![self.len()], self.clone())
    }
}

impl EinsumIn for [f64] {
    fn einsum_operand(&self) -> (Vec<usize>, Vec<f64>) {
        (vec![self.len()], self.to_vec())
    }
}

impl EinsumIn for Vec<Vec<f64>> {
    fn einsum_operand(&self) -> (Vec<usize>, Vec<f64>) {
        let rows = self.len();
        let cols = self.first().map(Vec::len).unwrap_or(0);
        let mut data = Vec::with_capacity(rows.saturating_mul(cols));
        for row in self {
            data.extend_from_slice(row);
        }
        (vec![rows, cols], data)
    }
}

impl EinsumIn for Tensor {
    fn einsum_operand(&self) -> (Vec<usize>, Vec<f64>) {
        (self.shape.clone(), self.data.clone())
    }
}

/// Output rank of an einsum subscript string (implicit mode included).
pub fn einsum_output_rank(subscripts: &str) -> usize {
    parse_einsum_subscripts(subscripts).1.chars().count()
}

/// Einstein summation over `(shape, row-major data)` operands.
/// Identities: `"ik,kj->ij"` is matmul; `"i,i->"` is dot; implicit
/// mode emits unique free indices alphabetically; `"i->ii"` is diag.
pub fn einsum_checked(
    subscripts: &str,
    operands: &[(Vec<usize>, Vec<f64>)],
) -> Result<(Vec<usize>, Vec<f64>), EinsumError> {
    let (input_specs, output_spec) = parse_einsum_subscripts(subscripts);
    if input_specs.len() != operands.len() {
        return Err(EinsumError::Arithmetic(
            "einsum operand count does not match subscripts",
        ));
    }

    let mut dim_sizes: std::collections::HashMap<char, usize> = std::collections::HashMap::new();
    for (spec, (shape, _)) in input_specs.iter().zip(operands.iter()) {
        if spec.len() != shape.len() {
            return Err(EinsumError::Arithmetic(
                "einsum operand rank does not match subscripts",
            ));
        }
        let mut local: std::collections::HashMap<char, usize> = std::collections::HashMap::new();
        for (letter, &size) in spec.chars().zip(shape.iter()) {
            match local.get(&letter) {
                Some(&prev) if prev != size => {
                    return Err(EinsumError::Arithmetic(
                        "einsum subscript repeats a letter at unequal extents",
                    ));
                }
                _ => {
                    local.insert(letter, size);
                }
            }
        }
        for (letter, size) in local {
            match dim_sizes.get(&letter) {
                None => {
                    dim_sizes.insert(letter, size);
                }
                Some(&prev) if prev == size || prev == 1 || size == 1 => {
                    dim_sizes.insert(letter, prev.max(size));
                }
                Some(_) => {
                    return Err(EinsumError::Arithmetic("einsum dimension mismatch"));
                }
            }
        }
    }

    for c in output_spec.chars() {
        if !dim_sizes.contains_key(&c) {
            return Err(EinsumError::Arithmetic(
                "einsum output index is not bound by any operand",
            ));
        }
    }

    let mut all_indices = Vec::new();
    for spec in input_specs.iter().chain(std::iter::once(&output_spec)) {
        for c in spec.chars() {
            if !all_indices.contains(&c) {
                all_indices.push(c);
            }
        }
    }
    let output_set: std::collections::HashSet<char> = output_spec.chars().collect();
    let contracted: Vec<char> = all_indices
        .iter()
        .copied()
        .filter(|c| !output_set.contains(c))
        .collect();

    let mut out_shape = Vec::with_capacity(output_spec.len());
    for c in output_spec.chars() {
        match dim_sizes.get(&c) {
            Some(&size) => out_shape.push(size),
            None => {
                return Err(EinsumError::Arithmetic(
                    "einsum output index is not bound by any operand",
                ));
            }
        }
    }
    let out_len: usize = if out_shape.is_empty() {
        1
    } else {
        out_shape.iter().product()
    };
    let mut out_data = vec![0.0f64; out_len];

    let contracted_sizes: Vec<usize> = contracted
        .iter()
        .map(|c| dim_sizes.get(c).copied().unwrap_or(1))
        .collect();
    let out_coords = cartesian_product(&out_shape);
    let contracted_coords = cartesian_product(&contracted_sizes);

    for (out_pos, out_coord) in out_coords.iter().enumerate() {
        let mut idx_map: std::collections::HashMap<char, usize> = std::collections::HashMap::new();
        let mut on_diagonal = true;
        for (i, c) in output_spec.chars().enumerate() {
            if let Some(&prev) = idx_map.get(&c) {
                if prev != out_coord[i] {
                    on_diagonal = false;
                    break;
                }
            } else {
                idx_map.insert(c, out_coord[i]);
            }
        }
        if !on_diagonal {
            continue;
        }
        let mut sum = 0.0f64;
        for c_coord in contracted_coords.iter() {
            for (i, c) in contracted.iter().enumerate() {
                idx_map.insert(*c, c_coord[i]);
            }
            let mut product = 1.0f64;
            for (spec, (shape, data)) in input_specs.iter().zip(operands.iter()) {
                let spec_chars: Vec<char> = spec.chars().collect();
                let mut flat_idx = 0usize;
                let mut stride = 1usize;
                for (c, &dim) in spec_chars.iter().zip(shape.iter()).rev() {
                    let idx = match idx_map.get(c) {
                        Some(_) if dim == 1 => 0,
                        Some(&i) if i < dim => i,
                        Some(&i) => {
                            return Err(EinsumError::IndexOutOfBounds {
                                index: i64::try_from(i).unwrap_or(i64::MAX),
                                len: dim,
                            });
                        }
                        None => {
                            return Err(EinsumError::Arithmetic(
                                "einsum output index is not bound by any operand",
                            ));
                        }
                    };
                    flat_idx += idx * stride;
                    stride *= dim.max(1);
                }
                let value = data
                    .get(flat_idx)
                    .copied()
                    .ok_or(EinsumError::IndexOutOfBounds {
                        index: i64::try_from(flat_idx).unwrap_or(i64::MAX),
                        len: data.len(),
                    })?;
                product *= value;
            }
            sum += product;
        }
        out_data[out_pos] = sum;
    }

    Ok((out_shape, out_data))
}

/// Rank-0 result.
pub fn einsum_scalar(out: (Vec<usize>, Vec<f64>)) -> f64 {
    out.1.first().copied().unwrap_or(0.0)
}

/// Rank-1 result.
pub fn einsum_vector(out: (Vec<usize>, Vec<f64>)) -> Vec<f64> {
    out.1
}

/// Rank-2 result as nested rows.
pub fn einsum_matrix(out: (Vec<usize>, Vec<f64>)) -> Vec<Vec<f64>> {
    let (shape, data) = out;
    let rows = shape.first().copied().unwrap_or(0);
    let cols = if shape.len() >= 2 { shape[1] } else { 0 };
    let mut matrix = Vec::with_capacity(rows);
    for r in 0..rows {
        let start = r.saturating_mul(cols);
        let end = start.saturating_add(cols);
        matrix.push(data.get(start..end).unwrap_or(&[]).to_vec());
    }
    matrix
}

/// Rank-3+ result (flat row-major, matching `TensorCreate` codegen).
pub fn einsum_tensor(out: (Vec<usize>, Vec<f64>)) -> Vec<f64> {
    out.1
}

fn einsum_or_panic(
    subscripts: &str,
    operands: &[(Vec<usize>, Vec<f64>)],
) -> (Vec<usize>, Vec<f64>) {
    match einsum_checked(subscripts, operands) {
        Ok(out) => out,
        Err(e) => panic!("{e}"),
    }
}

/// Panicking scalar einsum (generated crates; interp uses `einsum_checked`).
pub fn einsum_as_scalar(subscripts: &str, operands: &[(Vec<usize>, Vec<f64>)]) -> f64 {
    einsum_scalar(einsum_or_panic(subscripts, operands))
}

/// Panicking vector einsum.
pub fn einsum_as_vector(subscripts: &str, operands: &[(Vec<usize>, Vec<f64>)]) -> Vec<f64> {
    einsum_vector(einsum_or_panic(subscripts, operands))
}

/// Panicking matrix einsum.
pub fn einsum_as_matrix(subscripts: &str, operands: &[(Vec<usize>, Vec<f64>)]) -> Vec<Vec<f64>> {
    einsum_matrix(einsum_or_panic(subscripts, operands))
}

/// Panicking flat-tensor einsum.
pub fn einsum_as_tensor(subscripts: &str, operands: &[(Vec<usize>, Vec<f64>)]) -> Vec<f64> {
    einsum_tensor(einsum_or_panic(subscripts, operands))
}

fn parse_einsum_subscripts(s: &str) -> (Vec<String>, String) {
    let strip = |t: &str| t.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    if let Some((lhs, rhs)) = s.split_once("->") {
        let inputs: Vec<String> = lhs.split(',').map(strip).collect();
        (inputs, strip(rhs))
    } else {
        let inputs: Vec<String> = s.split(',').map(strip).collect();
        let output = implicit_einsum_output(&inputs);
        (inputs, output)
    }
}

fn implicit_einsum_output(input_specs: &[String]) -> String {
    let mut counts: std::collections::HashMap<char, usize> = std::collections::HashMap::new();
    for spec in input_specs {
        for c in spec.chars() {
            *counts.entry(c).or_insert(0) += 1;
        }
    }
    let mut output: Vec<char> = counts
        .into_iter()
        .filter(|&(_, n)| n == 1)
        .map(|(c, _)| c)
        .collect();
    output.sort_unstable();
    output.into_iter().collect()
}

fn cartesian_product(shape: &[usize]) -> Vec<Vec<usize>> {
    if shape.is_empty() {
        return vec![vec![]];
    }
    if shape.contains(&0) {
        return vec![];
    }
    let total: usize = shape.iter().product();
    let mut result = Vec::with_capacity(total);
    let mut current = vec![0usize; shape.len()];
    for _ in 0..total {
        result.push(current.clone());
        for i in (0..shape.len()).rev() {
            current[i] += 1;
            if current[i] < shape[i] {
                break;
            }
            current[i] = 0;
        }
    }
    result
}

// ── Stencils ──────────────────────────────────────────────────────────────

/// Edge policy for stencil convolution: how out-of-range taps are resolved.
pub enum EdgePolicy {
    /// Replicate the nearest in-range cell (zero-gradient / insulated).
    Clamp,
    /// Mirror across the boundary: u[-1] = u[1], u[n] = u[n-2]
    /// (second-order zero-gradient; trailing clamp guards tiny inputs).
    Neumann,
    /// Linearly extrapolate past the edge: u[-1] = 2u[0] − u[1].
    /// Combined with a central first-difference stencil this is a
    /// first-order one-sided difference, exact on linear fields.
    OneSided,
    /// Hold the boundary at fixed values.
    Dirichlet { left: f64, right: f64 },
}

/// Sample `input[raw]` with first-order one-sided extrapolation when `raw`
/// is out of range. A singleton field extrapolates as constant (gradient 0).
fn onesided_sample(input: &[f64], raw: isize) -> f64 {
    let n = input.len();
    if n == 0 {
        return 0.0;
    }
    let last = (n - 1) as isize;
    if raw >= 0 && raw <= last {
        input[raw as usize]
    } else if n == 1 {
        input[0]
    } else if raw < 0 {
        let k = -raw as f64;
        (1.0 + k) * input[0] - k * input[1]
    } else {
        let k = (raw - last) as f64;
        (1.0 + k) * input[n - 1] - k * input[n - 2]
    }
}

/// 2-D one-sided sample: extrapolate the out-of-range axis (or both).
fn onesided_sample_2d(input: &[Vec<f64>], raw_r: isize, raw_c: isize) -> f64 {
    let nr = input.len();
    if nr == 0 {
        return 0.0;
    }
    let row_at = |r: usize| onesided_sample(&input[r], raw_c);
    let last_r = (nr - 1) as isize;
    if raw_r >= 0 && raw_r <= last_r {
        row_at(raw_r as usize)
    } else if nr == 1 {
        row_at(0)
    } else if raw_r < 0 {
        let k = -raw_r as f64;
        (1.0 + k) * row_at(0) - k * row_at(1)
    } else {
        let k = (raw_r - last_r) as f64;
        (1.0 + k) * row_at(nr - 1) - k * row_at(nr - 2)
    }
}

/// 1D stencil convolution. `center` is the tap index that maps to the
/// output cell. Mirrors the historical inline semantics, including the
/// exact boundary math per edge policy.
pub fn stencil_1d(input: &[f64], weights: &[f64], center: i64, edge: EdgePolicy) -> Vec<f64> {
    let n = input.len();
    let last = n.saturating_sub(1) as isize;
    (0..n)
        .map(|i| {
            weights
                .iter()
                .enumerate()
                .map(|(k, &w)| {
                    let raw = i as isize + k as isize - center as isize;
                    match &edge {
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
                        EdgePolicy::OneSided => w * onesided_sample(input, raw),
                        EdgePolicy::Dirichlet { left, right } => {
                            if raw < 0 {
                                w * *left
                            } else if raw > last {
                                w * *right
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

/// 2D 3x3 stencil convolution; `center` is the (row, col) tap offset of
/// the center weight. Dirichlet is refused (mirrors codegen/interp).
pub fn stencil_2d(
    input: &[Vec<f64>],
    weights: &[f64; 9],
    center: (i64, i64),
    edge: EdgePolicy,
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
                    acc += match &edge {
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
                        EdgePolicy::OneSided => w * onesided_sample_2d(input, raw_r, raw_c),
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

fn extrapolation_terms(raw: isize, len: usize) -> [(usize, f64); 2] {
    if len <= 1 {
        return [(0, 1.0), (0, 0.0)];
    }
    let last = (len - 1) as isize;
    if raw < 0 {
        let k = -raw as f64;
        [(0, 1.0 + k), (1, -k)]
    } else if raw > last {
        let k = (raw - last) as f64;
        [(len - 1, 1.0 + k), (len - 2, -k)]
    } else {
        [(raw as usize, 1.0), (raw as usize, 0.0)]
    }
}

fn reflect_index(raw: isize, len: usize) -> usize {
    let last = len.saturating_sub(1) as isize;
    (if raw < 0 {
        -raw
    } else if raw > last {
        2 * last - raw
    } else {
        raw
    })
    .clamp(0, last) as usize
}

fn sample_3d(data: &[f64], shape: [usize; 3], raw: [isize; 3], edge: &EdgePolicy) -> f64 {
    let at = |x: usize, y: usize, z: usize| data[(x * shape[1] + y) * shape[2] + z];
    match edge {
        EdgePolicy::Clamp => at(
            raw[0].clamp(0, shape[0] as isize - 1) as usize,
            raw[1].clamp(0, shape[1] as isize - 1) as usize,
            raw[2].clamp(0, shape[2] as isize - 1) as usize,
        ),
        EdgePolicy::Neumann => at(
            reflect_index(raw[0], shape[0]),
            reflect_index(raw[1], shape[1]),
            reflect_index(raw[2], shape[2]),
        ),
        EdgePolicy::OneSided => {
            let xs = extrapolation_terms(raw[0], shape[0]);
            let ys = extrapolation_terms(raw[1], shape[1]);
            let zs = extrapolation_terms(raw[2], shape[2]);
            let mut value = 0.0;
            for (x, wx) in xs {
                for (y, wy) in ys {
                    for (z, wz) in zs {
                        value += wx * wy * wz * at(x, y, z);
                    }
                }
            }
            value
        }
        EdgePolicy::Dirichlet { .. } => {
            panic!("stencil_3d: Dirichlet boundary is not supported")
        }
    }
}

/// Checked rank-3 3x3x3 stencil convolution over a flat [`Tensor`].
///
/// Axes follow tensor shape order. Empty axes produce an empty tensor.
pub fn stencil_3d_slices_checked(
    input_shape: &[usize],
    input_data: &[f64],
    weights: &[f64; 27],
    center: (i64, i64, i64),
    edge: EdgePolicy,
) -> Result<Tensor, &'static str> {
    if input_shape.len() != 3 {
        return Err("3D stencil input must be a rank-3 tensor");
    }
    let shape = [input_shape[0], input_shape[1], input_shape[2]];
    let expected = shape[0]
        .checked_mul(shape[1])
        .and_then(|n| n.checked_mul(shape[2]))
        .ok_or("3D stencil tensor size overflow")?;
    if input_data.len() != expected {
        return Err("3D stencil data length does not match shape product");
    }
    if expected == 0 {
        return Ok(Tensor {
            shape: input_shape.to_vec(),
            data: Vec::new(),
        });
    }
    if matches!(edge, EdgePolicy::Dirichlet { .. }) {
        return Err("3D Dirichlet boundary is not yet supported");
    }

    let mut data = Vec::with_capacity(expected);
    for x in 0..shape[0] {
        for y in 0..shape[1] {
            for z in 0..shape[2] {
                let mut value = 0.0;
                for kx in 0..3 {
                    for ky in 0..3 {
                        for kz in 0..3 {
                            let weight = weights[(kx * 3 + ky) * 3 + kz];
                            let raw = [
                                x as isize + kx as isize - center.0 as isize,
                                y as isize + ky as isize - center.1 as isize,
                                z as isize + kz as isize - center.2 as isize,
                            ];
                            value += weight * sample_3d(input_data, shape, raw, &edge);
                        }
                    }
                }
                data.push(value);
            }
        }
    }
    Ok(Tensor {
        shape: input_shape.to_vec(),
        data,
    })
}

/// Checked rank-3 stencil over an owned-shape [`Tensor`].
pub fn stencil_3d_checked(
    input: &Tensor,
    weights: &[f64; 27],
    center: (i64, i64, i64),
    edge: EdgePolicy,
) -> Result<Tensor, &'static str> {
    stencil_3d_slices_checked(&input.shape, &input.data, weights, center, edge)
}

// ── Big-integer modular arithmetic (emath-t63iz stage 2) ─────────────────
//
// `UBig`: an arbitrary-precision NON-NEGATIVE integer, little-endian
// base-2^32 limbs, canonical (no high zero limbs). This is the stage-2
// representation for the six number-theory builtins (`int_rem`,
// `mod_inv`, `pow_mod`, `sqrt_mod`, `poly_eval_mod`, `rs_encode`):
// |F| < 2^256, exactly the production regime the stage-1 i64/i128 lane
// cannot reach. The algorithms are the stage-1 algorithms
// (square-and-multiply, Tonelli-Shanks, extended Euclid, Horner) over
// the swapped representation — a representation change, not an
// algorithm rewrite. The stage boundary stays explicit: admission
// refuses values ≥ 2^256 (see `LIMIT_BITS`) — widening the bound later
// is a constant change, not a redesign.
//
// This file is embedded verbatim into every generated crate (`SOURCE`
// in emath-rt's lib.rs), so generated Rust runs the SAME kernels as the
// interpreter — parity is structural, not hoped for. Determinism:
// every routine is integer-exact, allocation-pattern free of timing
// variation claims (no-claim: not constant-time; these are research
// probes, not crypto primitives).

/// Stage-2 value bound: |F| < 2^256.
pub const LIMIT_BITS: u32 = 256;

/// Canonical non-negative big integer: little-endian base-2^32 limbs
/// with no high zero limbs (zero is the empty limb vector).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UBig {
    limbs: Vec<u32>,
}

/// Kernel-level error for big modular arithmetic (stage-2 style mirrors
/// the `&'static str` refusals of the stage-1 kernels).
pub type BigError = &'static str;

impl UBig {
    /// Zero (canonical: empty limbs).
    pub fn zero() -> Self {
        UBig { limbs: Vec::new() }
    }

    /// One.
    pub fn one() -> Self {
        UBig { limbs: vec![1] }
    }

    /// From a u64.
    pub fn from_u64(value: u64) -> Self {
        let mut big = UBig {
            limbs: vec![value as u32, (value >> 32) as u32],
        };
        big.canonicalize();
        big
    }

    /// From an i64 by absolute value (`i64::MIN` → 2^63).
    pub fn from_i64_abs(value: i64) -> Self {
        UBig::from_u64(value.unsigned_abs())
    }

    /// From canonical little-endian u32 limbs (high zeros trimmed).
    pub fn from_limbs(mut limbs: Vec<u32>) -> Self {
        while limbs.last() == Some(&0) {
            limbs.pop();
        }
        UBig { limbs }
    }

    /// Canonical little-endian limbs (no high zeros; empty = zero).
    pub fn limbs(&self) -> &[u32] {
        &self.limbs
    }

    fn canonicalize(&mut self) {
        while self.limbs.last() == Some(&0) {
            self.limbs.pop();
        }
    }

    /// Exact decimal parse (no sign, no separators — the emitter strips
    /// `_` before calling). Refuses non-digits. Does NOT bound-check;
    /// the emitter enforces `LIMIT_BITS` at admission.
    pub fn parse_decimal(text: &str) -> Result<Self, BigError> {
        if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
            return Err("bigint literal must be a non-negative decimal integer");
        }
        let mut big = UBig::zero();
        let mut chunk_start = 0;
        while chunk_start < text.len() {
            let chunk_end = (chunk_start + 9).min(text.len());
            let chunk = &text[chunk_start..chunk_end];
            if chunk.is_empty() {
                break;
            }
            let value: u64 = chunk.parse().map_err(|_| "bigint literal chunk overflow")?;
            let scale = 10u64.pow((chunk_end - chunk_start) as u32);
            big.mul_small_add(value, scale);
            chunk_start = chunk_end;
        }
        Ok(big)
    }

    /// Exact decimal rendering (canonical, no leading zeros).
    pub fn to_decimal(&self) -> String {
        if self.limbs.is_empty() {
            return "0".to_string();
        }
        // Repeated division by 10^9; remainders are the digit groups.
        let mut chunks: Vec<u32> = Vec::new();
        let mut cur = self.limbs.clone();
        while !cur.is_empty() {
            let (quotient, remainder) = UBig::div_small(&cur, 1_000_000_000);
            chunks.push(remainder as u32);
            cur = quotient.limbs;
        }
        let mut text = String::new();
        for (index, chunk) in chunks.iter().enumerate().rev() {
            if index == chunks.len() - 1 {
                text.push_str(&chunk.to_string());
            } else {
                text.push_str(&format!("{chunk:09}"));
            }
        }
        text
    }

    /// Number of significant bits (0 for zero).
    pub fn bits(&self) -> u32 {
        match self.limbs.last() {
            None => 0,
            Some(&top) => (self.limbs.len() as u32 - 1) * 32 + (32 - top.leading_zeros()),
        }
    }

    /// True when the value is zero.
    pub fn is_zero(&self) -> bool {
        self.limbs.is_empty()
    }

    /// True when the value is one.
    pub fn is_one(&self) -> bool {
        self.limbs == [1]
    }

    /// Bit `i` of the little-endian bit string.
    fn bit(&self, i: u32) -> bool {
        let limb = i / 32;
        match self.limbs.get(limb as usize) {
            Some(&value) => (value >> (i % 32)) & 1 == 1,
            None => false,
        }
    }

    fn set_bit(&mut self, i: u32) {
        let limb = (i / 32) as usize;
        while self.limbs.len() <= limb {
            self.limbs.push(0);
        }
        self.limbs[limb] |= 1 << (i % 32);
    }

    pub fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        use core::cmp::Ordering;
        if self.limbs.len() != other.limbs.len() {
            return self.limbs.len().cmp(&other.limbs.len());
        }
        for i in (0..self.limbs.len()).rev() {
            match self.limbs[i].cmp(&other.limbs[i]) {
                Ordering::Equal => continue,
                other_order => return other_order,
            }
        }
        Ordering::Equal
    }

    pub fn add(&self, other: &Self) -> UBig {
        let mut limbs = Vec::with_capacity(self.limbs.len().max(other.limbs.len()) + 1);
        let mut carry = 0u64;
        for i in 0..self.limbs.len().max(other.limbs.len()) {
            let a = u64::from(*self.limbs.get(i).unwrap_or(&0));
            let b = u64::from(*other.limbs.get(i).unwrap_or(&0));
            let sum = a + b + carry;
            limbs.push(sum as u32);
            carry = sum >> 32;
        }
        if carry != 0 {
            limbs.push(carry as u32);
        }
        UBig { limbs }
    }

    /// `self - other` (caller guarantees `self ≥ other`).
    pub fn sub(&self, other: &Self) -> UBig {
        let mut limbs = Vec::with_capacity(self.limbs.len());
        let mut borrow = 0i64;
        for i in 0..self.limbs.len() {
            let a = i64::from(self.limbs[i]);
            let b = i64::from(*other.limbs.get(i).unwrap_or(&0)) + borrow;
            let (digit, new_borrow) = if a >= b { (a - b, 0) } else { (a + (1 << 32) - b, 1) };
            limbs.push(digit as u32);
            borrow = new_borrow;
        }
        let mut big = UBig { limbs };
        big.canonicalize();
        big
    }

    pub fn mul(&self, other: &Self) -> UBig {
        if self.limbs.is_empty() || other.limbs.is_empty() {
            return UBig::zero();
        }
        let mut limbs = vec![0u32; self.limbs.len() + other.limbs.len()];
        for (i, &a) in self.limbs.iter().enumerate() {
            let mut carry = 0u64;
            for (j, &b) in other.limbs.iter().enumerate() {
                let cur = u64::from(limbs[i + j]) + u64::from(a) * u64::from(b) + carry;
                limbs[i + j] = cur as u32;
                carry = cur >> 32;
            }
            limbs[i + other.limbs.len()] = carry as u32;
        }
        let mut big = UBig { limbs };
        big.canonicalize();
        big
    }

    /// Multiply by a small u64 and add a small u64 (parse helper; both
    /// operands < 2^32-scale so u64 intermediates never overflow).
    fn mul_small_add(&mut self, small: u64, scale: u64) {
        let mut carry = small;
        for limb in &mut self.limbs {
            let cur = u64::from(*limb) * scale + carry;
            *limb = cur as u32;
            carry = cur >> 32;
        }
        while carry != 0 {
            self.limbs.push(carry as u32);
            carry >>= 32;
        }
    }

    /// Divide by a small u64: returns (quotient, remainder).
    pub fn div_small(limbs: &[u32], divisor: u64) -> (UBig, u64) {
        let mut quotient = vec![0u32; limbs.len()];
        let mut rem = 0u64;
        for i in (0..limbs.len()).rev() {
            let cur = (rem << 32) | u64::from(limbs[i]);
            quotient[i] = (cur / divisor) as u32;
            rem = cur % divisor;
        }
        let mut big = UBig { limbs: quotient };
        big.canonicalize();
        (big, rem)
    }

    /// Shift left by one bit (`self * 2`).
    pub fn shl1(&self) -> UBig {
        let mut limbs = Vec::with_capacity(self.limbs.len() + 1);
        let mut carry = 0u32;
        for &limb in &self.limbs {
            limbs.push((limb << 1) | carry);
            carry = limb >> 31;
        }
        if carry != 0 {
            limbs.push(carry);
        }
        let mut big = UBig { limbs };
        big.canonicalize();
        big
    }

    /// `(a + b) mod m` for `a, b < m` — subtraction instead of a
    /// double-width add.
    fn add_mod(a: &UBig, b: &UBig, m: &UBig) -> UBig {
        // a, b < m < 2^256 ⇒ a + b < 2^257: one extra limb suffices.
        let sum = a.add(b);
        if sum.cmp(m) != core::cmp::Ordering::Less {
            sum.sub(m)
        } else {
            sum
        }
    }

    /// `(a - b) mod m` for `a, b < m` — add m when the raw difference
    /// would be negative (never materialized).
    fn sub_mod(a: &UBig, b: &UBig, m: &UBig) -> UBig {
        if a.cmp(b) != core::cmp::Ordering::Less {
            a.sub(b)
        } else {
            m.sub(b).add(a)
        }
    }

    /// `a * b mod m` via the full product then one binary reduction.
    pub fn mul_mod(a: &UBig, b: &UBig, m: &UBig) -> UBig {
        UBig::rem(&a.mul(b), m)
    }

    /// Binary long division: `(a / b, a mod b)`. `b == 0` is the
    /// caller's typed refusal (mirrors int_rem's positive-modulus
    /// contract); bit-shift subtract keeps u32 limbs exact for
    /// 512-bit stage-2 products.
    fn rem(a: &UBig, b: &UBig) -> UBig {
        debug_assert!(!b.is_zero());
        if a.cmp(b) == core::cmp::Ordering::Less {
            return a.clone();
        }
        let mut remainder = UBig::zero();
        for i in (0..a.bits()).rev() {
            remainder = remainder.shl1();
            if a.bit(i) {
                remainder.set_bit(0);
            }
            if remainder.cmp(b) != core::cmp::Ordering::Less {
                remainder = remainder.sub(b);
            }
        }
        remainder
    }

    /// `base^exp mod m` (square-and-multiply over the big
    /// representation; same algorithm as the stage-1 i128 kernel).
    fn mod_pow(base: &UBig, exp: &UBig, m: &UBig) -> UBig {
        let mut result = UBig::rem(&UBig::one(), m);
        let mut b = UBig::rem(base, m);
        for i in (0..exp.bits()).rev() {
            result = UBig::mul_mod(&result, &result, m);
            if exp.bit(i) {
                result = UBig::mul_mod(&result, &b, m);
            }
        }
        result
    }

    /// `(v: i64) promoted into [0, m)`: sign-correct Euclidean
    /// placement without any i128 modulus cast (m may be ≥ 2^127).
    pub fn from_i64_rem(value: i64, m: &UBig) -> UBig {
        let magnitude = UBig::from_i64_abs(value);
        let rem = UBig::rem(&magnitude, m);
        if value >= 0 {
            rem
        } else if rem.is_zero() {
            rem
        } else {
            m.sub(&rem)
        }
    }
}

/// `a rem_euclid m` over `UBig` (stage-2 int_rem kernel). `a` is a
/// canonical non-negative big value; the i64-negative case promotes
/// through `from_i64_rem`.
pub fn big_int_rem_checked(a: &UBig, m: &UBig) -> Result<UBig, BigError> {
    if m.is_zero() {
        return Err("int_rem: modulus must be non-zero");
    }
    Ok(UBig::rem(a, m))
}

/// `a rem_euclid m` with a signed i64 `a` and a big modulus.
pub fn big_int_rem_i64_checked(a: i64, m: &UBig) -> Result<UBig, BigError> {
    if m.is_zero() {
        return Err("int_rem: modulus must be non-zero");
    }
    Ok(UBig::from_i64_rem(a, m))
}

/// Modular inverse via the iterative extended Euclidean algorithm with
/// Bezout coefficients kept in `[0, m)` (same algorithm as the stage-1
/// `mod_inv_checked`; the representation is all that changed).
pub fn big_mod_inv_checked(a: &UBig, m: &UBig) -> Result<UBig, BigError> {
    if m.is_zero() {
        return Err("mod_inv: modulus must be positive");
    }
    let a = UBig::rem(a, m);
    if a.is_zero() {
        return Err("mod_inv: no inverse exists (gcd != 1)");
    }
    let mut r0 = m.clone();
    let mut r1 = a;
    let mut t0 = UBig::zero();
    let mut t1 = UBig::one();
    while !r1.is_zero() {
        let (quotient, remainder) = big_div_rem(&r0, &r1);
        r0 = r1;
        r1 = remainder;
        // t ← (t0 - q·t1) mod m, staying in [0, m).
        let q_t = UBig::mul_mod(&quotient, &t1, m);
        let next_t = UBig::sub_mod(&t0, &q_t, m);
        t0 = core::mem::replace(&mut t1, next_t);
    }
    if r0.is_one() {
        Ok(t0)
    } else {
        Err("mod_inv: no inverse exists (gcd != 1)")
    }
}

/// Full binary long division: `(a / b, a mod b)`.
pub fn big_div_rem(a: &UBig, b: &UBig) -> (UBig, UBig) {
    let mut quotient = UBig::zero();
    let mut remainder = UBig::zero();
    for i in (0..a.bits()).rev() {
        remainder = remainder.shl1();
        if a.bit(i) {
            remainder.set_bit(0);
        }
        if remainder.cmp(b) != core::cmp::Ordering::Less {
            remainder = remainder.sub(b);
            quotient.set_bit(i);
        }
    }
    (quotient, remainder)
}

/// `base^exp mod m` (stage-2 pow_mod kernel).
pub fn big_pow_mod_checked(base: &UBig, exp: &UBig, m: &UBig) -> Result<UBig, BigError> {
    if m.is_zero() {
        return Err("pow_mod: modulus must be positive");
    }
    Ok(UBig::mod_pow(base, exp, m))
}

/// Tonelli-Shanks square root in F_p over the stage-2 representation.
/// Same law set as stage-1 `sqrt_mod_checked`: odd prime `p` (2 handled
/// inline), deterministic smallest non-residue, `min(x, p - x)`
/// tie-break, and the exactness gate that doubles as the typed
/// non-residue refusal.
pub fn big_sqrt_mod_checked(a: &UBig, p: &UBig) -> Result<UBig, BigError> {
    if p.is_zero() {
        return Err("sqrt_mod: modulus must be positive");
    }
    let two = UBig::from_u64(2);
    if p.cmp(&two) == core::cmp::Ordering::Equal {
        return Ok(UBig::rem(a, p));
    }
    if p.bit(0) == false {
        return Err("sqrt_mod: modulus must be an odd prime (2 handled above)");
    }
    let modulus = UBig::rem(a, p);
    if modulus.is_zero() {
        return Ok(UBig::zero());
    }
    // Fast path: p ≡ 3 (mod 4) → x = a^((p+1)/4).
    let one = UBig::one();
    let four = UBig::from_u64(4);
    let p_mod_4 = UBig::rem(p, &four);
    let mut x = if p_mod_4 == UBig::from_u64(3) {
        let exp = p.add(&one).div_u64(4);
        UBig::mod_pow(&modulus, &exp, p)
    } else {
        // Legendre pre-check (emath-t63iz, found by the wide-mod tests):
        // the Tonelli-Shanks loop below assumes `a` is a residue — for a
        // non-residue the least-i search reaches i = m and the shift
        // m - i - 1 underflows. Refuse here; the exactness gate below
        // still backstops non-prime p.
        let p_minus_1 = p.sub(&one);
        if UBig::mod_pow(&modulus, &p_minus_1.div_u64(2), p) != one {
            return Err("sqrt_mod: no square root exists (a is a non-residue or p is not prime)");
        }
        // General Tonelli-Shanks: p - 1 = q·2^s with q odd.
        let p_minus_1 = p.sub(&one);
        let mut q = p_minus_1.clone();
        let mut s: u64 = 0;
        while q.bit(0) == false {
            q = q.div_u64(2);
            s += 1;
        }
        // Deterministic non-residue search (smallest z with
        // Legendre symbol -1; always exists for prime p).
        let half = p_minus_1.div_u64(2);
        let mut z = UBig::from_u64(2);
        let pm1 = p.sub(&one);
        loop {
            if UBig::mod_pow(&z, &half, p) == pm1 {
                break;
            }
            z = z.add(&one);
        }
        let mut m = s;
        let mut c = UBig::mod_pow(&z, &q, p);
        let mut t = UBig::mod_pow(&modulus, &q, p);
        let mut r = UBig::mod_pow(&modulus, &q.add(&one).div_u64(2), p);
        while !(t.is_one()) {
            // Least i with t^(2^i) = 1.
            let mut i: u64 = 0;
            let mut tt = t.clone();
            while !(tt.is_one()) {
                tt = UBig::mul_mod(&tt, &tt, p);
                i += 1;
            }
            // b = c^(2^(m-i-1)).
            let shift = m - i - 1;
            let mut b = c.clone();
            for _ in 0..shift {
                b = UBig::mul_mod(&b, &b, p);
            }
            m = i;
            c = UBig::mul_mod(&b, &b, p);
            t = UBig::mul_mod(&t, &c, p);
            r = UBig::mul_mod(&r, &b, p);
        }
        r
    };
    // Defensive exactness gate: a fabricated root must never escape
    // (this is also the typed refusal path for quadratic non-residues).
    if UBig::mul_mod(&x, &x, p) != modulus {
        return Err("sqrt_mod: no square root exists (a is a non-residue or p is not prime)");
    }
    let mirror = p.sub(&x);
    if x.cmp(&mirror) == core::cmp::Ordering::Greater {
        x = mirror;
    }
    Ok(x)
}

impl UBig {
    /// Divide by a small u64 (helper for the Tonelli-Shanks shifts).
    pub fn div_u64(&self, divisor: u64) -> UBig {
        UBig::div_small(&self.limbs, divisor).0
    }

    /// Integer value when the big value fits `i64` (result-side
    /// narrowing for callers that stay in the stage-1 lane).
    pub fn to_i64(&self) -> Option<i64> {
        if self.bits() > 63 {
            return None;
        }
        let mut value: u64 = 0;
        for (index, &limb) in self.limbs.iter().enumerate() {
            value |= u64::from(limb) << (32 * index as u64);
        }
        i64::try_from(value).ok()
    }
}

/// Polynomial evaluation over GF(p) by Horner's method with big `x`/`p`
/// (coefficients stay on the f64 surface, exact ≤ 2^53; the Horner
/// products are the stage-2 wide step).
pub fn big_poly_eval_mod_checked(
    coeffs: &[f64],
    x: &UBig,
    p: &UBig,
) -> Result<UBig, BigError> {
    if p.is_zero() {
        return Err("poly_eval_mod: modulus must be positive");
    }
    let mut result = UBig::zero();
    for &c in coeffs.iter().rev() {
        let coefficient = exact_i64_coeff(c)?;
        result = UBig::mul_mod(&result, x, p);
        result = add_i64_mod(&result, coefficient, p);
    }
    Ok(result)
}

/// Reed-Solomon codeword over the big modulus: evaluate at x = 0..n
/// through the shared big Horner kernel.
pub fn big_rs_encode_checked(coeffs: &[f64], n: i64, p: &UBig) -> Result<Vec<UBig>, BigError> {
    if p.is_zero() {
        return Err("rs_encode: modulus must be positive");
    }
    if n <= 0 || UBig::from_u64(n as u64).cmp(p) != core::cmp::Ordering::Less {
        return Err("rs_encode: codeword length n must be in (0, p)");
    }
    let mut codeword = Vec::with_capacity(n as usize);
    for x in 0..n {
        codeword.push(big_poly_eval_mod_checked(coeffs, &UBig::from_u64(x as u64), p)?);
    }
    Ok(codeword)
}

/// `(r + c) mod m` for a signed i64 coefficient.
fn add_i64_mod(r: &UBig, c: i64, m: &UBig) -> UBig {
    if c >= 0 {
        UBig::add_mod(r, &UBig::from_u64(c as u64), m)
    } else {
        UBig::sub_mod(r, &UBig::from_i64_abs(c), m)
    }
}

/// `as i64` on the f64 coefficient surface: NaN→refused, Inf→refused,
/// fractional→refused. Integer kernels refuse silent finite lies.
fn exact_i64_coeff(value: f64) -> Result<i64, BigError> {
    if !value.is_finite() || value.fract() != 0.0 {
        return Err("coefficient must be a finite whole number");
    }
    if value < i64::MIN as f64 || value > i64::MAX as f64 {
        return Err("coefficient exceeds i64 range");
    }
    Ok(value as i64)
}

// ── Codegen-facing wrappers (emath-t63iz stage 2) ────────────────────────
//
// Generated Rust runs only ADMITTED programs, so a refusal here is an
// internal invariant violation: the panic posture matches the i64
// wrappers in `numeric.rs` (interpreter refusals stay typed; the
// generated lane never observes a refused input).

/// Panicking `int_rem` for generated Rust (admission guarantees `m > 0`).
pub fn big_int_rem(a: &UBig, m: &UBig) -> UBig {
    big_int_rem_checked(a, m).expect("int_rem refusal leaked past admission")
}

/// Panicking `mod_inv` for generated Rust (admission guarantees
/// invertibility or an interpreter-visible fault).
pub fn big_mod_inv(a: &UBig, m: &UBig) -> UBig {
    big_mod_inv_checked(a, m).expect("mod_inv refusal leaked past admission")
}

/// Panicking `pow_mod` for generated Rust.
pub fn big_pow_mod(base: &UBig, exp: &UBig, m: &UBig) -> UBig {
    big_pow_mod_checked(base, exp, m).expect("pow_mod refusal leaked past admission")
}

/// Panicking `sqrt_mod` for generated Rust (admission guarantees a
/// residue base and an odd prime modulus).
pub fn big_sqrt_mod(a: &UBig, p: &UBig) -> UBig {
    big_sqrt_mod_checked(a, p).expect("sqrt_mod refusal leaked past admission")
}

/// Panicking `poly_eval_mod` for generated Rust.
pub fn big_poly_eval_mod(coeffs: &[f64], x: &UBig, p: &UBig) -> UBig {
    big_poly_eval_mod_checked(coeffs, x, p).expect("poly_eval_mod refusal leaked past admission")
}

/// Panicking `rs_encode` for generated Rust.
pub fn big_rs_encode(coeffs: &[f64], n: i64, p: &UBig) -> Vec<UBig> {
    big_rs_encode_checked(coeffs, n, p).expect("rs_encode refusal leaked past admission")
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

// ── Graph traversal ──────────────────────────────
//
// Deterministic graph algorithms over a DENSE adjacency carrier: nested
// row-major `adj[i][j]` = edge `i → j` (0.0 = no edge; nonzero = edge
// whose value is the weight — the generated-crate matrix
// representation, dims carried by the structure). Vertices are indices
// and every neighbor scan is ascending-index: no hash-order
// nondeterminism anywhere. INVALID input (non-square carrier, empty
// traversal carrier, non-whole or out-of-range source, negative weight
// under Dijkstra's precondition) yields an EMPTY result — the typed
// refusal surface of the reference interpreter maps these to
// E-GRAPH-001..003; the generated code observes the same refusals as
// deterministic empty outputs. Kernels never panic.

/// Square dimensions of a uniform nested carrier (None when ragged or
/// empty).
fn graph_dims(adj: &[Vec<f64>]) -> Option<(usize, usize)> {
    let rows = adj.len();
    if rows == 0 {
        return None;
    }
    let cols = adj[0].len();
    if cols != rows || adj.iter().any(|row| row.len() != cols) {
        return None;
    }
    Some((rows, cols))
}

/// Whole vertex index in `0..n` from an f64 source (None otherwise).
fn graph_source_index(source: f64, n: usize) -> Option<usize> {
    if !source.is_finite() || source.fract() != 0.0 || source < 0.0 {
        return None;
    }
    let index = source as usize;
    if index >= n {
        return None;
    }
    Some(index)
}

/// BFS reachability mask from `source`: element `v` is 1.0 when `v` is
/// reachable from `source` (the source reaches itself), else 0.0.
/// Empty on invalid carrier/source.
pub fn graph_reachable(adj: &[Vec<f64>], source: f64) -> Vec<f64> {
    let Some((n, _)) = graph_dims(adj) else {
        return Vec::new();
    };
    let Some(source) = graph_source_index(source, n) else {
        return Vec::new();
    };
    let mut mask = vec![0.0; n];
    mask[source] = 1.0;
    let mut queue = vec![source];
    let mut head = 0usize;
    while head < queue.len() {
        let u = queue[head];
        head += 1;
        for v in 0..n {
            if adj[u][v] != 0.0 && mask[v] == 0.0 {
                mask[v] = 1.0;
                queue.push(v);
            }
        }
    }
    mask
}

/// BFS visit order from `source` (source first, neighbors discovered in
/// ascending index — breadth-first, never depth-first, never
/// insertion-order). Empty on invalid carrier/source.
pub fn graph_bfs_order(adj: &[Vec<f64>], source: f64) -> Vec<f64> {
    let Some((n, _)) = graph_dims(adj) else {
        return Vec::new();
    };
    let Some(source) = graph_source_index(source, n) else {
        return Vec::new();
    };
    let mut visited = vec![false; n];
    visited[source] = true;
    let mut order = vec![source as f64];
    let mut queue = vec![source];
    let mut head = 0usize;
    while head < queue.len() {
        let u = queue[head];
        head += 1;
        for v in 0..n {
            if adj[u][v] != 0.0 && !visited[v] {
                visited[v] = true;
                order.push(v as f64);
                queue.push(v);
            }
        }
    }
    order
}

/// O(n²) selection Dijkstra: shortest distances from `source` over the
/// nonnegative weights of the carrier (0.0 = no edge). Unreachable
/// vertices are +Inf — honest numeric, never a wrong finite distance.
/// Empty on invalid carrier/source or a NEGATIVE weight (Dijkstra's
/// precondition; negative edges are a named deferral, not a silent
/// wrong answer).
pub fn graph_dijkstra(adj: &[Vec<f64>], source: f64) -> Vec<f64> {
    let Some((n, _)) = graph_dims(adj) else {
        return Vec::new();
    };
    let Some(source) = graph_source_index(source, n) else {
        return Vec::new();
    };
    if adj.iter().any(|row| row.iter().any(|w| *w < 0.0)) {
        return Vec::new();
    }
    let infinity = f64::INFINITY;
    let mut distances = vec![infinity; n];
    distances[source] = 0.0;
    let mut settled = vec![false; n];
    for _ in 0..n {
        // Deterministic tie-break: the LOWEST-index unsettled vertex
        // with the minimum distance.
        let Some(u) = (0..n)
            .filter(|v| !settled[*v])
            .min_by(|x, y| distances[*x].total_cmp(&distances[*y]).then(x.cmp(y)))
        else {
            break;
        };
        if distances[u].is_infinite() {
            break; // remaining vertices are unreachable
        }
        settled[u] = true;
        for v in 0..n {
            let weight = adj[u][v];
            if weight != 0.0 {
                let candidate = distances[u] + weight;
                if candidate < distances[v] {
                    distances[v] = candidate;
                }
            }
        }
    }
    distances
}

/// Out-degree per vertex: count of NONZERO entries in the row (0.0 is
/// no edge even in a weighted carrier; a self-loop counts). In-degree
/// is this op over the transposed carrier. Empty on a ragged carrier;
/// an empty carrier has no degrees (empty result).
pub fn graph_degree_out(adj: &[Vec<f64>]) -> Vec<f64> {
    match graph_dims(adj) {
        Some((_n, cols)) => adj
            .iter()
            .map(|row| row[..cols].iter().filter(|w| **w != 0.0).count() as f64)
            .collect(),
        None if adj.is_empty() => Vec::new(),
        None => Vec::new(),
    }
}

// ── Nested-carrier adapters (spectral/iterative kernels) ───────────
//
// The generated crate carries matrices as nested `Vec<Vec<f64>>` (see
// `mat_transpose`); these adapters flatten into the flat kernels above
// so backend emission can call the same algorithmic core. Empty
// carrier → empty result (the kernels' typed-refusal surface).

fn flatten_square_or_none(m: &[Vec<f64>]) -> Option<(Vec<f64>, usize, usize)> {
    let rows = m.len();
    if rows == 0 {
        return None;
    }
    let cols = m[0].len();
    if m.iter().any(|row| row.len() != cols) {
        return None;
    }
    let flat: Vec<f64> = m.iter().flatten().copied().collect();
    Some((flat, rows, cols))
}

/// Eigenvalues (ascending) of a real symmetric square nested matrix;
/// empty on refusal.
pub fn eig_values(m: &[Vec<f64>]) -> Vec<f64> {
    match flatten_square_or_none(m) {
        Some((flat, rows, cols)) => eig_values_flat(&flat, rows, cols),
        None => Vec::new(),
    }
}

/// Eigenvector matrix (flat row-major, column j for eigenvalue j);
/// empty on refusal.
pub fn eig_vectors(m: &[Vec<f64>]) -> Vec<f64> {
    match flatten_square_or_none(m) {
        Some((flat, rows, cols)) => eig_vectors_flat(&flat, rows, cols),
        None => Vec::new(),
    }
}

/// Singular values (descending) of a rectangular nested matrix; empty
/// on refusal.
pub fn svd_values(m: &[Vec<f64>]) -> Vec<f64> {
    let rows = m.len();
    if rows == 0 {
        return Vec::new();
    }
    let cols = m[0].len();
    if m.iter().any(|row| row.len() != cols) {
        return Vec::new();
    }
    let flat: Vec<f64> = m.iter().flatten().copied().collect();
    svd_values_flat(&flat, rows, cols)
}

/// Packed `[U; s; Vᵀ]` thin-SVD factors of a nested matrix; empty on
/// refusal.
pub fn svd_factors(m: &[Vec<f64>]) -> Vec<f64> {
    let rows = m.len();
    if rows == 0 {
        return Vec::new();
    }
    let cols = m[0].len();
    if m.iter().any(|row| row.len() != cols) {
        return Vec::new();
    }
    let flat: Vec<f64> = m.iter().flatten().copied().collect();
    svd_factors_flat(&flat, rows, cols)
}

/// Conjugate gradient over a nested SPD matrix: solves `A x = b`;
/// empty on refusal (non-convergence or shape mismatch).
pub fn cg_solve(a: &[Vec<f64>], b: &[f64]) -> Vec<f64> {
    let rows = a.len();
    if rows == 0 {
        return Vec::new();
    }
    let cols = a[0].len();
    if a.iter().any(|row| row.len() != cols) || b.len() != rows {
        return Vec::new();
    }
    let flat: Vec<f64> = a.iter().flatten().copied().collect();
    cg_solve_flat(&flat, rows, cols, b)
}

/// Dense partial-pivot solve over a nested square matrix.
pub fn linear_solve(a: &[Vec<f64>], b: &[f64]) -> Vec<f64> {
    match flatten_square_or_none(a) {
        Some((flat, rows, cols)) => linear_solve_flat(&flat, rows, cols, b),
        None => Vec::new(),
    }
}

/// Packed `[p; L; U]` partial-pivot LU factors.
pub fn lu_factors(a: &[Vec<f64>]) -> Vec<Vec<f64>> {
    let Some((flat, rows, cols)) = flatten_square_or_none(a) else {
        return Vec::new();
    };
    let packed = lu_factors_flat(&flat, rows, cols);
    packed.chunks(cols).map(<[f64]>::to_vec).collect()
}

/// Packed `[Q; R]` thin QR factors.
pub fn qr_factors(a: &[Vec<f64>]) -> Vec<Vec<f64>> {
    let rows = a.len();
    let Some(cols) = a.first().map(Vec::len) else {
        return Vec::new();
    };
    if a.iter().any(|row| row.len() != cols) {
        return Vec::new();
    }
    let flat = a.iter().flatten().copied().collect::<Vec<_>>();
    let packed = qr_factors_flat(&flat, rows, cols);
    packed.chunks(cols).map(<[f64]>::to_vec).collect()
}

/// Outer product `a bᵀ`.
pub fn outer_product(a: &[f64], b: &[f64]) -> Vec<Vec<f64>> {
    if a.is_empty() || b.is_empty() || a.iter().chain(b).any(|value| !value.is_finite()) {
        return Vec::new();
    }
    a.iter()
        .map(|left| b.iter().map(|right| left * right).collect())
        .collect()
}

/// Unnormalized graph Laplacian over a DENSE adjacency carrier:
/// `L = D − A` where `D` is the out-degree diagonal (nonzero-entry
/// counts, the degree law) and `A` is the carrier. The Laplacian
/// of an UNDIRECTED graph (symmetric adjacency) is symmetric, so its
/// spectrum composes through the symmetric eigen kernel; a directed
/// carrier's Laplacian is not symmetric and the eigen gate refuses it
/// upstream (the documented class fence, never a silent symmetrization).
/// Empty on an invalid carrier (the typed-refusal surface upstream).
pub fn graph_laplacian(adj: &[Vec<f64>]) -> Vec<Vec<f64>> {
    let Some((n, cols)) = graph_dims(adj) else {
        return Vec::new();
    };
    let mut laplacian = vec![vec![0.0; cols]; n];
    for (row_index, row) in adj.iter().enumerate() {
        let degree: f64 = row[..cols].iter().filter(|w| **w != 0.0).count() as f64;
        for (col_index, weight) in row[..cols].iter().enumerate() {
            laplacian[row_index][col_index] =
                if row_index == col_index { degree } else { 0.0 } - weight;
        }
    }
    laplacian
}

// ── Linear programming + multi-objective ──────────────────────────────────
//
// Deterministic optimization kernels over dense carriers. The LP is the
// STANDARD-FORM class: minimize cᵀx s.t. A x ≤ b, x ≥ 0, b ≥ 0 — the
// origin slack basis is feasible, so infeasibility cannot arise here
// (negative right-side normalization is a named deferral). Pivoting is
// Bland's rule (smallest-index entering column, smallest-index basis
// tie-break in the ratio test): provably terminating, no cycling, no
// hash-order anything. Empty output = typed refusal upstream; kernels
// never panic and never return a wrong "optimum".

/// Simplex pivot tolerance.
const LP_PIVOT_TOLERANCE: f64 = 1e-12;
/// Bland's rule terminates; this cap is a bug guard, never a policy.
const LP_ITERATION_CAP: usize = 100_000;

/// Bland's-rule simplex for the standard-form class above. Returns the
/// optimal x (length n), or EMPTY on invalid dimensions, non-finite
/// entries, b < 0 (non-standard form), or an unbounded objective.
pub fn lp_minimize(a: &[Vec<f64>], b: &[f64], c: &[f64]) -> Vec<f64> {
    let m = a.len();
    if m == 0 || b.len() != m {
        return Vec::new();
    }
    let n = c.len();
    if n == 0 || a.iter().any(|row| row.len() != n) {
        return Vec::new();
    }
    if a.iter().any(|row| row.iter().any(|v| !v.is_finite()))
        || b.iter().any(|v| !v.is_finite())
        || c.iter().any(|v| !v.is_finite())
    {
        return Vec::new();
    }
    if b.iter().any(|v| *v < 0.0) {
        return Vec::new();
    }
    // Tableau: [A | I | b] with the slack basis; column `width` is the
    // right-hand side.
    let width = n + m;
    let mut tableau = vec![vec![0.0; width + 1]; m];
    for (i, row) in a.iter().enumerate() {
        tableau[i][..n].copy_from_slice(row);
        tableau[i][n + i] = 1.0;
        tableau[i][width] = b[i];
    }
    // Extended cost: c on the structural variables, 0 on the slacks.
    let mut cost = vec![0.0; width];
    cost[..n].copy_from_slice(c);
    let mut basis: Vec<usize> = (n..n + m).collect();
    for _ in 0..LP_ITERATION_CAP {
        // Reduced costs from scratch (deterministic, basis-explicit):
        // r_j = c_j − Σ_i cost[basis[i]] · T[i][j].
        let mut reduced = vec![0.0; width];
        for j in 0..width {
            let mut acc = cost[j];
            for i in 0..m {
                acc -= cost[basis[i]] * tableau[i][j];
            }
            reduced[j] = acc;
        }
        // Bland's entering rule: the SMALLEST index with a negative
        // reduced cost.
        let Some(enter) = (0..width).find(|j| reduced[*j] < -LP_PIVOT_TOLERANCE) else {
            // Optimal: extract x from the basis.
            let mut x = vec![0.0; n];
            for (i, variable) in basis.iter().enumerate() {
                if *variable < n {
                    x[*variable] = tableau[i][width];
                }
            }
            return x;
        };
        // Ratio test: minimum increase; Bland's tie-break is the
        // smallest basis-variable index among the tied rows.
        let mut leaving: Option<usize> = None;
        let mut best_ratio = f64::INFINITY;
        for i in 0..m {
            let pivot = tableau[i][enter];
            if pivot > LP_PIVOT_TOLERANCE {
                let ratio = tableau[i][width] / pivot;
                if ratio < best_ratio - LP_PIVOT_TOLERANCE
                    || ((ratio - best_ratio).abs() <= LP_PIVOT_TOLERANCE
                        && matches!(leaving, Some(current) if basis[i] < basis[current]))
                {
                    best_ratio = ratio;
                    leaving = Some(i);
                }
            }
        }
        let Some(row) = leaving else {
            return Vec::new(); // unbounded: typed upstream
        };
        // Pivot on (row, enter).
        let pivot = tableau[row][enter];
        for entry in tableau[row].iter_mut() {
            *entry /= pivot;
        }
        for i in 0..m {
            if i != row {
                let factor = tableau[i][enter];
                if factor != 0.0 {
                    for j in 0..=width {
                        tableau[i][j] -= factor * tableau[row][j];
                    }
                }
            }
        }
        basis[row] = enter;
    }
    Vec::new() // unreachable under Bland's rule; bug guard
}

/// Non-dominated mask over a finite carrier of objective vectors (all
/// MINIMIZED; maximize by negating — the documented convention).
/// STRICT Pareto: identical points do not dominate each other, so both
/// stay on the front. Point order = mask order (the portfolio
/// artifact's deterministic data). Empty on an empty/ragged carrier or
/// a non-finite entry.
pub fn pareto_front(points: &[Vec<f64>]) -> Vec<f64> {
    let Some(k) = points.first().map(Vec::len) else {
        return Vec::new();
    };
    if k == 0 || points.iter().any(|point| point.len() != k) {
        return Vec::new();
    }
    if points.iter().any(|p| p.iter().any(|v| !v.is_finite())) {
        return Vec::new();
    }
    let mut mask = vec![1.0; points.len()];
    for (i, point) in points.iter().enumerate() {
        for (j, other) in points.iter().enumerate() {
            if i == j {
                continue;
            }
            // `other` dominates `point`: componentwise ≤ AND strictly
            // better somewhere.
            let weakly = other.iter().zip(point.iter()).all(|(o, p)| o <= p);
            let strictly = other.iter().zip(point.iter()).any(|(o, p)| o < p);
            if weakly && strictly {
                mask[i] = 0.0;
                break;
            }
        }
    }
    mask
}

// ── Polynomials as values ───────────────
//
// Dense coefficient vectors, ASCENDING order (index i = coefficient of
// xⁱ). The EMPTY vector is the zero polynomial (additive identity) —
// documented algebra, never a shape error. Deterministic strict-f64:
// ascending-index convolution, one-pass Horner.

/// Cauchy convolution of two coefficient vectors (ascending order):
/// `c[i+j] += a[i]·b[j]`. An empty operand is the zero polynomial
/// (empty product).
pub fn poly_mul(a: &[f64], b: &[f64]) -> Vec<f64> {
    if a.is_empty() || b.is_empty() {
        return Vec::new();
    }
    let mut c = vec![0.0; a.len() + b.len() - 1];
    for (i, ai) in a.iter().enumerate() {
        for (j, bj) in b.iter().enumerate() {
            c[i + j] += ai * bj;
        }
    }
    c
}

/// Horner evaluation of a coefficient vector (ascending order) at
/// `point`. Empty coefficients evaluate to 0.0 (the zero polynomial).
pub fn poly_eval(coefficients: &[f64], point: f64) -> f64 {
    let mut value = 0.0;
    for coefficient in coefficients.iter().rev() {
        value = value * point + coefficient;
    }
    value
}

/// Coefficients `0..=budget` of a homogeneous linear recurrence.
pub fn sequence_generate(initial: &[f64], recurrence: &[f64], budget: f64) -> Vec<f64> {
    if !budget.is_finite()
        || budget < 0.0
        || budget.fract() != 0.0
        || budget > 1_000_000.0
        || initial.is_empty()
        || recurrence.is_empty()
        || recurrence.len() > initial.len()
        || budget as usize + 1 < initial.len()
        || initial
            .iter()
            .chain(recurrence)
            .any(|value| !value.is_finite())
    {
        return Vec::new();
    }
    let budget = budget as usize;
    let mut values = initial.to_vec();
    while values.len() <= budget {
        let n = values.len();
        let next = recurrence
            .iter()
            .enumerate()
            .map(|(offset, coefficient)| coefficient * values[n - offset - 1])
            .sum::<f64>();
        if !next.is_finite() {
            return Vec::new();
        }
        values.push(next);
    }
    values
}

/// First `count` coefficients of the Cauchy product of two finite series.
pub fn sequence_convolve(left: &[f64], right: &[f64], count: f64) -> Vec<f64> {
    if !count.is_finite()
        || count < 0.0
        || count.fract() != 0.0
        || count > 1_000_000.0
        || count as usize > left.len().saturating_add(right.len()).saturating_sub(1)
        || left.iter().chain(right).any(|value| !value.is_finite())
    {
        return Vec::new();
    }
    let mut result = vec![0.0; count as usize];
    for (index, output) in result.iter_mut().enumerate() {
        let first = index.saturating_sub(right.len().saturating_sub(1));
        let last = index.min(left.len().saturating_sub(1));
        for left_index in first..=last {
            *output += left[left_index] * right[index - left_index];
        }
        if !output.is_finite() {
            return Vec::new();
        }
    }
    result
}

// ── Stiff + symplectic ODE nucleus────────────────────
//
// Scalar-ODE carriers with ASCENDING polynomial rate laws
// (`rate(y) = Σ c[i]·yⁱ`): deterministic strict-f64 kernels. Backward
// Euler solves the implicit equation with damped Newton (closed
// iteration budget, machine-tolerance residual); velocity Verlet is
// the kick-drift-kick for separable Hamiltonian form (one force
// evaluation per step, time-reversible). Empty output = typed refusal
// upstream; kernels never panic and never return a wrong trajectory
// point.

/// Evaluate an ascending polynomial rate law at `y` (Horner).
fn poly_rate(coefficients: &[f64], y: f64) -> f64 {
    let mut value = 0.0;
    for coefficient in coefficients.iter().rev() {
        value = value * y + coefficient;
    }
    value
}

/// One backward-Euler step for a scalar ODE `y' = rate(y)`:
/// solve `y1 = y0 + h·rate(y1)` with Newton (analytic derivative of
/// the polynomial rate; forward-difference fallback is unnecessary —
/// the derivative is exact). Returns EMPTY on non-finite input,
/// non-positive `h`, or Newton non-convergence (typed upstream —
/// never a silently wrong step).
pub fn ode_backward_euler_step(rate_coefficients: &[f64], y0: f64, h: f64) -> Vec<f64> {
    if rate_coefficients.is_empty()
        || !rate_coefficients.iter().all(|c| c.is_finite())
        || !y0.is_finite()
        || !h.is_finite()
        || h <= 0.0
    {
        return Vec::new();
    }
    let rate = |y: f64| poly_rate(rate_coefficients, y);
    // dRate/dy (ascending polynomial derivative).
    let derivative_coefficients: Vec<f64> = (1..rate_coefficients.len())
        .map(|i| rate_coefficients[i] * i as f64)
        .collect();
    let derivative = |y: f64| {
        if derivative_coefficients.is_empty() {
            0.0
        } else {
            poly_rate(&derivative_coefficients, y)
        }
    };
    // Initial guess: the explicit step (first order, converges for
    // decaying modes; the Newton damping covers stiff transients).
    let mut x = y0 + h * rate(y0);
    for _ in 0..50 {
        let residual = x - h * rate(x) - y0;
        let slope = 1.0 - h * derivative(x);
        if slope.abs() < 1e-300 {
            return Vec::new();
        }
        let delta = residual / slope;
        x -= delta;
        if delta.abs() <= 1e-13 * (x.abs() + 1.0) {
            // Converged: verify the residual at machine tolerance.
            if (x - h * rate(x) - y0).abs() <= 1e-10 {
                return vec![x];
            }
            return Vec::new();
        }
    }
    Vec::new()
}

/// One velocity-Verlet step for the separable system `q' = v`,
/// `v' = a(q)` (acceleration as an ascending polynomial of position):
/// kick-drift-kick. `h` may be negative (time reversal — the
/// symplectic law). Returns `[q1, v1]`, or EMPTY on non-finite input
/// or non-finite `h`.
pub fn ode_velocity_verlet_step(
    acceleration_coefficients: &[f64],
    q0: f64,
    v0: f64,
    h: f64,
) -> Vec<f64> {
    if acceleration_coefficients.is_empty()
        || !acceleration_coefficients.iter().all(|c| c.is_finite())
        || !q0.is_finite()
        || !v0.is_finite()
        || !h.is_finite()
        || h == 0.0
    {
        return Vec::new();
    }
    let acceleration = |q: f64| poly_rate(acceleration_coefficients, q);
    let a0 = acceleration(q0);
    let q1 = q0 + v0 * h + 0.5 * a0 * h * h;
    let a1 = acceleration(q1);
    let v1 = v0 + 0.5 * (a0 + a1) * h;
    vec![q1, v1]
}

// ── Spectral Poisson, 1D Dirichlet────────────────────
//
// The 3-point Laplacian on a uniform interior grid of [0,1] with
// Dirichlet boundaries diagonalizes EXACTLY in the DST-I sine basis:
// eigenvector v_k(j) = sin(πjk/(n+1)) carries the POSITIVE eigenvalue
// λ_k = (4/h²)·sin²(kπ/(2(n+1))) of −Δ_h. The solve −Δ_h u = f is a
// forward sine transform of the load, division by the λ_k, and the
// inverse transform — deterministic O(n²) strict-f64, no iteration,
// no new solver machinery. Empty or non-finite loads return EMPTY
// (typed upstream; never a silently wrong field).

pub fn poisson_dirichlet_sine(load: &[f64]) -> Vec<f64> {
    let n = load.len();
    if n == 0 || !load.iter().all(|value| value.is_finite()) {
        return Vec::new();
    }
    let n_f = n as f64;
    let h = 1.0 / (n_f + 1.0);
    // Positive Dirichlet eigenvalues of −Δ_h, mode k = 1..=n.
    let eigenvalues: Vec<f64> = (1..=n)
        .map(|k| {
            let theta = std::f64::consts::PI * k as f64 * h / 2.0;
            let sine = theta.sin();
            (4.0 / (h * h)) * sine * sine
        })
        .collect();
    // Forward DST-I (unnormalized) of the interior load.
    let coefficients: Vec<f64> = (1..=n)
        .map(|k| {
            load.iter()
                .enumerate()
                .map(|(j, f)| {
                    f * (std::f64::consts::PI * (j as f64 + 1.0) * k as f64 / (n_f + 1.0)).sin()
                })
                .sum()
        })
        .collect();
    // Diagonal division, then the inverse DST-I with the 2/(n+1) norm.
    (1..=n)
        .map(|j| {
            coefficients
                .iter()
                .zip(eigenvalues.iter())
                .enumerate()
                .map(|(k, (f_k, lambda))| {
                    f_k / lambda
                        * (std::f64::consts::PI * j as f64 * (k as f64 + 1.0) / (n_f + 1.0)).sin()
                })
                .sum::<f64>()
                * (2.0 / (n_f + 1.0))
        })
        .collect()
}

// ── Probability: seeded sampling + densities─────────
//
// ONE generator, one place: SplitMix64 (the compute-layer nucleus the
// stream contract composes above — no second RNG namespace).
// The seed is an f64 scalar whose to_bits() initializes the state
// (PROVISIONAL mapping; re-mappable without
// touching the generators). Uniform01 via the high 53 bits. Normal
// via Box–Muller (one pair per draw, u1 remapped off zero).
// Deterministic strict-f64 throughout: same seed ⟹ bit-identical
// draws.

/// One SplitMix64 step: stateful, deterministic, high-quality output
/// for seeding and streams alike.
pub fn splitmix64_next(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Uniform f64 in [0, 1) from one u64 (high 53 bits).
fn prob_uniform01(state: &mut u64) -> f64 {
    let bits = splitmix64_next(state) >> 11;
    (bits as f64) * (1.0 / (1u64 << 53) as f64)
}

/// Sample `draws` values from the named distribution (ascending param
/// carriers: Normal `[mu, sigma]`, Uniform `[a, b]`, Bernoulli `[p]`).
/// Returns EMPTY on any invalid input (typed upstream — never a
/// silently wrong stream).
pub fn prob_sample(kind: u8, params: &[f64], seed: f64, draws: usize) -> Vec<f64> {
    let arity_ok = match kind {
        0 | 1 => params.len() == 2,
        2 => params.len() == 1,
        _ => false,
    };
    let finite = params.iter().all(|p| p.is_finite()) && seed.is_finite();
    let param_ok = match kind {
        0 => params.get(1).is_some_and(|sigma| *sigma > 0.0),
        1 => match (params.first(), params.get(1)) {
            (Some(a), Some(b)) => a <= b,
            _ => false,
        },
        2 => params.first().is_some_and(|p| (0.0..=1.0).contains(p)),
        _ => false,
    };
    if !arity_ok || !finite || !param_ok || draws == 0 || draws > 1 << 20 {
        return Vec::new();
    }
    let mut state = seed.to_bits();
    match kind {
        // Normal(μ, σ): Box–Muller, one (u1, u2) pair per draw.
        0 => {
            let (mu, sigma) = (params[0], params[1]);
            (0..draws)
                .map(|_| {
                    let u1 = 1.0 - prob_uniform01(&mut state); // (0, 1]
                    let u2 = prob_uniform01(&mut state);
                    let magnitude = (-2.0 * u1.ln()).sqrt();
                    mu + sigma * magnitude * (2.0 * std::f64::consts::PI * u2).cos()
                })
                .collect()
        }
        // Uniform(a, b): affine map of [0, 1).
        1 => {
            let (a, b) = (params[0], params[1]);
            (0..draws)
                .map(|_| a + prob_uniform01(&mut state) * (b - a))
                .collect()
        }
        // Bernoulli(p): threshold one uniform; p ∈ {0, 1} exact.
        2 => {
            let p = params[0];
            (0..draws)
                .map(|_| {
                    if prob_uniform01(&mut state) < p {
                        1.0
                    } else {
                        0.0
                    }
                })
                .collect()
        }
        _ => Vec::new(),
    }
}

/// Density / PMF of the named distribution at `x` (ascending param
/// carriers as in `prob_sample`). Returns EMPTY (as `Option::None`
/// upstream) on invalid input; the density is exact, not estimated.
pub fn prob_density(kind: u8, params: &[f64], x: f64) -> Option<f64> {
    let arity_ok = match kind {
        0 | 1 => params.len() == 2,
        2 => params.len() == 1,
        _ => false,
    };
    let finite = params.iter().all(|p| p.is_finite()) && x.is_finite();
    let param_ok = match kind {
        0 => params.get(1).is_some_and(|sigma| *sigma > 0.0),
        1 => match (params.first(), params.get(1)) {
            (Some(a), Some(b)) => a <= b,
            _ => false,
        },
        2 => params.first().is_some_and(|p| (0.0..=1.0).contains(p)),
        _ => false,
    };
    if !arity_ok || !finite || !param_ok {
        return None;
    }
    match kind {
        // Normal: (1 / (σ√(2π)))·exp(−(x−μ)²/(2σ²)).
        0 => {
            let (mu, sigma) = (params[0], params[1]);
            let z = (x - mu) / sigma;
            let exponent = -0.5 * z * z;
            let normalization = 1.0 / (sigma * (2.0 * std::f64::consts::PI).sqrt());
            // exp(−large) underflows to 0.0 — a true density value.
            Some(normalization * exponent.exp())
        }
        // Uniform: 1/(b−a) on [a, b], 0 outside.
        1 => {
            let (a, b) = (params[0], params[1]);
            if (a..=b).contains(&x) {
                Some(1.0 / (b - a))
            } else {
                Some(0.0)
            }
        }
        // Bernoulli PMF: p at 1, 1−p at 0, 0 elsewhere.
        2 => {
            let p = params[0];
            if x == 1.0 {
                Some(p)
            } else if x == 0.0 {
                Some(1.0 - p)
            } else {
                Some(0.0)
            }
        }
        _ => None,
    }
}

// ── Graph symmetrization (directed → spectral path) ────────
//
// S = (A + Aᵀ)/2 — the weight-preserving symmetrization convention
// (documented; NOT max, NOT boolean-or). The output is a symmetric
// adjacency, so the existing laplacian + symmetric-eigen path applies;
// symmetrization is a USER choice, never a silent one inside
// laplacian/eigen. Empty on ragged/empty carriers or any non-finite
// weight (typed upstream; never a silently wrong carrier).

pub fn graph_symmetrize(adj: &[Vec<f64>]) -> Vec<Vec<f64>> {
    match graph_dims(adj) {
        Some((n, cols)) if n == cols => {
            if adj
                .iter()
                .any(|row| row.iter().any(|weight| !weight.is_finite()))
            {
                return Vec::new();
            }
            (0..n)
                .map(|i| (0..n).map(|j| (adj[i][j] + adj[j][i]) / 2.0).collect())
                .collect()
        }
        _ => Vec::new(),
    }
}

// ── Bellman-Ford: negative-edge shortest paths ────────────
//
// Classic O(n·m) relaxation over the dense carrier: n−1 passes of
// ascending-index edge relaxation, then a final detection pass. Negative
// weights are ADMITTED (the point — Dijkstra's greedy invariant is what
// fails here); a negative cycle REACHABLE from the source means no
// shortest-path answer exists → EMPTY (typed E-GRAPH-005 upstream, never
// fabricated distances). Unreachable vertices are +Inf (honest numeric).
// Deterministic: relaxation order is fixed (source index ascending),
// identical inputs bit-identical.

pub fn graph_bellman_ford(adj: &[Vec<f64>], source: usize) -> Vec<f64> {
    let Some((n, cols)) = graph_dims(adj) else {
        return Vec::new();
    };
    if n != cols
        || source >= n
        || adj
            .iter()
            .any(|row| row.iter().any(|weight| !weight.is_finite()))
    {
        return Vec::new();
    }
    let mut distances = vec![f64::INFINITY; n];
    distances[source] = 0.0;
    for _ in 0..n.saturating_sub(1) {
        let mut changed = false;
        for u in 0..n {
            if !distances[u].is_finite() {
                continue;
            }
            for v in 0..n {
                let weight = adj[u][v];
                if weight == 0.0 {
                    continue;
                }
                let candidate = distances[u] + weight;
                if candidate < distances[v] {
                    distances[v] = candidate;
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    // Detection pass: any further improvement ⇒ a negative cycle
    // reachable from the source.
    for u in 0..n {
        if !distances[u].is_finite() {
            continue;
        }
        for v in 0..n {
            let weight = adj[u][v];
            if weight != 0.0 && distances[u] + weight < distances[v] {
                return Vec::new();
            }
        }
    }
    distances
}

// ── Sparse storage: COO triplet carrier ───────────────────
//
// The thin storage nucleus over the dense graph path: extraction and
// build, both deterministic. Explicit 0.0 entries are NOT edges (the
// dense convention) and are skipped on extraction; DUPLICATE (u, v)
// entries SUM on build (the COO law — parallel edges add weights).
// Empty on ragged carriers / malformed streams / out-of-range indices
// / non-finite weights (typed upstream; never a silently wrong
// carrier).

pub fn graph_sparse_triplets(adj: &[Vec<f64>]) -> Vec<f64> {
    let Some((n, cols)) = graph_dims(adj) else {
        return Vec::new();
    };
    if n != cols
        || adj
            .iter()
            .any(|row| row.iter().any(|weight| !weight.is_finite()))
    {
        return Vec::new();
    }
    let mut triplets = Vec::new();
    for (u, row) in adj.iter().enumerate() {
        for (v, weight) in row[..cols].iter().enumerate() {
            if *weight != 0.0 {
                triplets.push(u as f64);
                triplets.push(v as f64);
                triplets.push(*weight);
            }
        }
    }
    triplets
}

pub fn graph_sparse_from_triplets(n: usize, triplets: &[f64]) -> Vec<Vec<f64>> {
    if triplets.len() % 3 != 0 {
        return Vec::new();
    }
    let mut adj = vec![vec![0.0; n]; n];
    for fields in triplets.chunks_exact(3) {
        let [u, v, weight] = fields else {
            return Vec::new();
        };
        let (Some(u), Some(v)) = (graph_source_index(*u, n), graph_source_index(*v, n)) else {
            return Vec::new();
        };
        if !weight.is_finite() {
            return Vec::new();
        }
        adj[u][v] += weight;
    }
    adj
}

// ── Thin control surface (transfer / state-space / stability) ──────
//
// ASCENDING polynomial carriers (the B28 representation law); the
// state-space carrier is a square row-major A with implicit D = 0.
// Raw kernels are TOTAL with documented degenerate returns (NaN for
// scalar refusals; the typed refusals E-CONTROL-001..005 live in the
// `control` wrapper module and the reference interpreter surfaces
// them). Controller DESIGN (pole placement, LQR) and the Itô/
// Stratonovich SDE surface (B37, world-dependent) are NOT claimed
// here. Determinism class: fixed-order Horner, fixed-order
// Faddeev–LeVerrier recurrence, partial-pivot Gauss elimination with
// FIRST-INDEX tie-breaking; identical inputs are bit-identical.

/// Routh–Hurwitz table status for a real polynomial (ASCENDING
/// coefficients). `Stable` = every root strictly in the open left half
/// plane; `Unstable` = at least one first-column sign change (a
/// right-half-plane root exists); `Degenerate` = a zero first-column
/// entry (marginal poles or the ε-ambiguous case — the
/// auxiliary-polynomial refinement is a named deferral);
/// `ZeroPolynomial` = no pole set; `NonFinite` = a non-finite
/// coefficient.
pub enum RouthStatus {
    Stable,
    Unstable,
    Degenerate,
    ZeroPolynomial,
    NonFinite,
}

/// Routh–Hurwitz first-column sign test over an ASCENDING carrier.
/// Leading-coefficient sign is normalized away (stability is invariant
/// under an overall sign flip); a constant nonzero polynomial has an
/// EMPTY pole set and is vacuously stable.
pub fn control_routh_status(den: &[f64]) -> RouthStatus {
    if den.iter().any(|c| !c.is_finite()) {
        return RouthStatus::NonFinite;
    }
    // Descending order for the table; strip leading zeros (trailing
    // zeros of the ASCENDING carrier) to find the true degree.
    let mut desc: Vec<f64> = den.iter().rev().copied().collect();
    while desc.last().is_some_and(|c| *c == 0.0) {
        desc.pop();
    }
    if desc.is_empty() {
        return RouthStatus::ZeroPolynomial;
    }
    if desc.len() == 1 {
        return RouthStatus::Stable;
    }
    if desc[0] < 0.0 {
        for c in desc.iter_mut() {
            *c = -*c;
        }
    }
    let mut row0: Vec<f64> = desc.iter().step_by(2).copied().collect();
    let mut row1: Vec<f64> = desc.iter().skip(1).step_by(2).copied().collect();
    let mut prev_positive = row0[0] > 0.0;
    loop {
        if row1.is_empty() {
            return RouthStatus::Stable;
        }
        // A zero first-column entry is the degenerate (marginal or
        // ε-ambiguous) case — refused typed upstream, never guessed.
        if row1[0] == 0.0 {
            return RouthStatus::Degenerate;
        }
        if (row1[0] > 0.0) != prev_positive {
            return RouthStatus::Unstable;
        }
        prev_positive = row1[0] > 0.0;
        // The new row has row0.len() - 1 entries; missing operands read
        // as 0.0 (the classical padded-table convention).
        let at = |row: &[f64], k: usize| if k < row.len() { row[k] } else { 0.0 };
        let width = row0.len();
        let mut next = Vec::with_capacity(width.saturating_sub(1));
        for k in 0..width.saturating_sub(1) {
            next.push((row1[0] * at(&row0, k + 1) - row0[0] * at(&row1, k + 1)) / row1[0]);
        }
        row0 = row1;
        row1 = next;
    }
}

/// Total bool view of the Routh status (generated-code convention:
/// degenerate/zero/non-finite all read false; the typed wrapper
/// refuses them upstream).
pub fn control_poles_stable(den: &[f64]) -> bool {
    matches!(control_routh_status(den), RouthStatus::Stable)
}

/// Characteristic polynomial of `A` (monic, ASCENDING coefficients)
/// via the Faddeev–LeVerrier recurrence: `M₁ = I`,
/// `a_{n−k} = −tr(A·M_k)/k`, `M_{k+1} = A·M_k + a_{n−k}·I`. Pure
/// matrix arithmetic — no eigenvalue is claimed.
pub fn control_char_poly(a: &[Vec<f64>]) -> Vec<f64> {
    let n = a.len();
    let mut m_prev = vec![0.0; n * n];
    let mut descending: Vec<f64> = vec![1.0];
    for k in 1..=n {
        let previous = descending[descending.len() - 1];
        let mut m_k = vec![0.0; n * n];
        for r in 0..n {
            for c in 0..n {
                let mut acc = 0.0;
                for j in 0..n {
                    acc += a[r][j] * m_prev[j * n + c];
                }
                m_k[r * n + c] = acc + if r == c { previous } else { 0.0 };
            }
        }
        let mut trace = 0.0;
        for d in 0..n {
            for j in 0..n {
                trace += a[d][j] * m_k[j * n + d];
            }
        }
        descending.push(-trace / k as f64);
        m_prev = m_k;
    }
    descending.reverse();
    descending
}

/// Transfer-function evaluation `num(x)/den(x)` over ASCENDING
/// carriers (Horner both sides). NaN = refused (zero denominator,
/// pole hit, or non-finite carrier) — the typed wrapper names it.
pub fn control_transfer_eval(num: &[f64], den: &[f64], x: f64) -> f64 {
    let denominator = poly_eval(den, x);
    if !denominator.is_finite()
        || denominator == 0.0
        || !num
            .iter()
            .chain(den.iter())
            .chain(std::iter::once(&x))
            .all(|v| v.is_finite())
    {
        return f64::NAN;
    }
    poly_eval(num, x) / denominator
}

/// State-space DC gain `c·(−A)⁻¹·b` (implicit D = 0). Refuses (NaN)
/// unless the Faddeev–LeVerrier characteristic polynomial is strictly
/// stable (Routh–Hurwitz): a non-asymptotically-stable carrier has no
/// DC gain. The solve is pivoted Gauss elimination with first-index
/// tie-breaking.
pub fn control_state_space_dc_gain(a: &[Vec<f64>], b: &[f64], c: &[f64]) -> f64 {
    let n = a.len();
    if n == 0
        || a.iter().any(|row| row.len() != n)
        || b.len() != n
        || c.len() != n
        || a.iter()
            .flatten()
            .chain(b.iter())
            .chain(c.iter())
            .any(|v| !v.is_finite())
    {
        return f64::NAN;
    }
    if !matches!(
        control_routh_status(&control_char_poly(a)),
        RouthStatus::Stable
    ) {
        return f64::NAN;
    }
    // Solve (−A)x = b by pivoted Gauss elimination (ties → first row:
    // the strictly-greater comparison never swaps an equal pivot).
    let mut aug: Vec<Vec<f64>> = (0..n)
        .map(|r| {
            let mut row: Vec<f64> = (0..n).map(|cc| -a[r][cc]).collect();
            row.push(b[r]);
            row
        })
        .collect();
    for col in 0..n {
        let mut pivot = col;
        for r in col + 1..n {
            if aug[r][col].abs() > aug[pivot][col].abs() {
                pivot = r;
            }
        }
        if aug[pivot][col] == 0.0 {
            // Unreachable for a strictly stable carrier (det ≠ 0);
            // refused rather than invented.
            return f64::NAN;
        }
        aug.swap(col, pivot);
        for r in col + 1..n {
            let factor = aug[r][col] / aug[col][col];
            if factor != 0.0 {
                for cc in col..=n {
                    aug[r][cc] -= factor * aug[col][cc];
                }
            }
        }
    }
    let mut x = vec![0.0; n];
    for r in (0..n).rev() {
        let mut acc = aug[r][n];
        for cc in r + 1..n {
            acc -= aug[r][cc] * x[cc];
        }
        x[r] = acc / aug[r][r];
    }
    let mut gain = 0.0;
    for (ci, xi) in c.iter().zip(x.iter()) {
        gain += ci * xi;
    }
    gain
}

// ── Finite-category surface ─────────────────────────
//
// The kernel: a finite
// `(dom, cod, comp)` — per-morphism object indices plus a DENSE k×k
// composition table. `comp[i][j] = m_i ∘ m_j` (j FIRST, then i) and is
// defined exactly when `cod[j] == dom[i]`; `-1.0` marks undefined. The
// composite's dom/cod are `dom[j]`/`cod[i]`. Objects are implicit
// `0..n`, `n = max(dom ∪ cod) + 1`. Equal morphism INDEX means equal
// morphism. Diagrams are face path-pairs: each face record is
// `[start, end, len_l, len_r, left…, right…]` (both paths ≥ 1
// morphism) and is commutative iff both path composites are the SAME
// morphism index.
//
// Category laws (composition totality/alignment, identity existence,
// associativity) are CERTIFIED by the gate before any commutativity
// answer — never assumed. Raw kernels are TOTAL with documented
// degenerate returns; the typed refusals E-CAT-001..007 live in the
// `category` wrapper module. Determinism class: fixed-order law
// passes, first-failure refusal, index-fold path evaluation;
// identical inputs are bit-identical.

/// Upper bound on morphisms for which associativity is certified by
/// the exhaustive triple check (64³ table probes). Larger carriers
/// refuse `E-CAT-007` — commutativity is never answered over an
/// unverified table.
pub const CATEGORY_ASSOCIATIVITY_BOUND: usize = 64;

/// Category-law gate status for a dense composition-table carrier.
/// `Valid` = the carrier is a category; the other variants name the
/// FIRST violated law in the documented pass order.
pub enum CategoryStatus {
    Valid,
    /// `E-CAT-001` — a non-finite entry anywhere in the carrier.
    NonFinite,
    /// `E-CAT-002` — shape: dimension mismatch, malformed face record,
    /// or a path that does not run its face's declared start→end.
    BadShape,
    /// `E-CAT-003` — an out-of-range or non-integral index.
    BadIndex,
    /// `E-CAT-004` — composition law: an aligned pair without an
    /// entry, a defined entry on a misaligned pair, or a dangling
    /// path segment.
    EntryLaw,
    /// `E-CAT-005` — identity law: an appearing object with no
    /// identity morphism.
    IdentityLaw,
    /// `E-CAT-006` — associativity law (or definedness disagreement).
    AssociativityLaw,
    /// `E-CAT-007` — more morphisms than the certifiable bound.
    TooLarge,
}

/// Parse one f64 field as an index in `0..bound`. Call only AFTER the
/// finiteness pass (NaN/non-finite refuse `E-CAT-001` before this).
fn category_index(value: f64, bound: usize) -> Option<usize> {
    if value < 0.0 || value.fract() != 0.0 {
        return None;
    }
    let index = value as usize;
    (index < bound).then_some(index)
}

/// The certified carrier: parsed object indices plus the composition
/// table as `i64` (-1 = undefined).
struct CertifiedCategory {
    dom: Vec<usize>,
    cod: Vec<usize>,
    table: Vec<Vec<i64>>,
    objects: usize,
}

/// The category-law gate (documented pass order: shape → finiteness →
/// indices → size bound → composition law → identity law →
/// associativity). Returns the parsed carrier on success.
fn category_certify(
    dom: &[f64],
    cod: &[f64],
    comp: &[Vec<f64>],
) -> Result<CertifiedCategory, CategoryStatus> {
    let k = dom.len();
    if k != cod.len() || comp.len() != k || comp.iter().any(|row| row.len() != k) {
        return Err(CategoryStatus::BadShape);
    }
    if dom
        .iter()
        .chain(cod.iter())
        .chain(comp.iter().flatten())
        .any(|value| !value.is_finite())
    {
        return Err(CategoryStatus::NonFinite);
    }
    let mut objects = 0usize;
    for value in dom.iter().chain(cod.iter()) {
        let index = category_index(*value, usize::MAX).ok_or(CategoryStatus::BadIndex)?;
        objects = objects.max(index + 1);
    }
    let mut table = vec![vec![-1i64; k]; k];
    for (i, row) in comp.iter().enumerate() {
        for (j, value) in row.iter().enumerate() {
            if *value == -1.0 {
                continue;
            }
            let entry = category_index(*value, k).ok_or(CategoryStatus::BadIndex)?;
            table[i][j] = entry as i64;
        }
    }
    // Size gate before the quadratic/cubic law passes: a carrier too
    // large to certify is refused outright, never half-checked.
    if k > CATEGORY_ASSOCIATIVITY_BOUND {
        return Err(CategoryStatus::TooLarge);
    }
    let dom_i: Vec<usize> = dom.iter().map(|v| *v as usize).collect();
    let cod_i: Vec<usize> = cod.iter().map(|v| *v as usize).collect();
    // Composition law: defined exactly on aligned pairs, and the
    // composite carries the pair's dom/cod.
    for i in 0..k {
        for j in 0..k {
            let aligned = cod_i[j] == dom_i[i];
            let entry = table[i][j];
            if aligned {
                if entry < 0 {
                    return Err(CategoryStatus::EntryLaw);
                }
                let composite = entry as usize;
                if dom_i[composite] != dom_i[j] || cod_i[composite] != cod_i[i] {
                    return Err(CategoryStatus::EntryLaw);
                }
            } else if entry >= 0 {
                return Err(CategoryStatus::EntryLaw);
            }
        }
    }
    // Identity law: every APPEARING object has a morphism that acts as
    // its identity on both sides.
    let mut appears = vec![false; objects];
    for object in dom_i.iter().chain(cod_i.iter()) {
        appears[*object] = true;
    }
    for object in 0..objects {
        if !appears[object] {
            continue;
        }
        let mut found = false;
        'candidate: for m in 0..k {
            if dom_i[m] != object || cod_i[m] != object || table[m][m] != m as i64 {
                continue;
            }
            for x in 0..k {
                if cod_i[x] == object && table[m][x] != x as i64 {
                    continue 'candidate;
                }
                if dom_i[x] == object && table[x][m] != x as i64 {
                    continue 'candidate;
                }
            }
            found = true;
            break;
        }
        if !found {
            return Err(CategoryStatus::IdentityLaw);
        }
    }
    // Associativity: exhaustive triple check (definedness already
    // agrees with alignment under the composition law, so a
    // one-side-defined disagreement is also a violation).
    for a in 0..k {
        for b in 0..k {
            for c in 0..k {
                let ab = table[a][b];
                let bc = table[b][c];
                let left = if ab >= 0 { table[ab as usize][c] } else { -1 };
                let right = if bc >= 0 { table[a][bc as usize] } else { -1 };
                if left != right {
                    return Err(CategoryStatus::AssociativityLaw);
                }
            }
        }
    }
    Ok(CertifiedCategory {
        dom: dom_i,
        cod: cod_i,
        table,
        objects,
    })
}

/// The law-gate status view (the wrapper's typed surface).
pub fn category_check_status(dom: &[f64], cod: &[f64], comp: &[Vec<f64>]) -> CategoryStatus {
    match category_certify(dom, cod, comp) {
        Ok(_) => CategoryStatus::Valid,
        Err(status) => status,
    }
}

/// Total check view (generated-code convention): TRUE only when the
/// carrier certifies; every law failure reads false (the reference
/// interpreter surfaces the typed E-CAT codes).
pub fn category_check(dom: &[f64], cod: &[f64], comp: &[Vec<f64>]) -> bool {
    matches!(category_check_status(dom, cod, comp), CategoryStatus::Valid)
}

/// Diagram commutativity over face path-pairs (status view): the
/// carrier must certify first, then each face's two paths fold through
/// the table; a face is commutative iff both composites are the SAME
/// morphism index.
pub fn category_diagram_commutative_status(
    dom: &[f64],
    cod: &[f64],
    comp: &[Vec<f64>],
    faces: &[f64],
) -> Result<Vec<bool>, CategoryStatus> {
    let category = category_certify(dom, cod, comp)?;
    let k = dom.len();
    if faces.iter().any(|value| !value.is_finite()) {
        return Err(CategoryStatus::NonFinite);
    }
    let mut mask = Vec::new();
    let mut cursor = 0usize;
    while cursor < faces.len() {
        if cursor + 4 > faces.len() {
            return Err(CategoryStatus::BadShape);
        }
        let start =
            category_index(faces[cursor], category.objects).ok_or(CategoryStatus::BadIndex)?;
        let end =
            category_index(faces[cursor + 1], category.objects).ok_or(CategoryStatus::BadIndex)?;
        let len_l_raw = faces[cursor + 2];
        let len_r_raw = faces[cursor + 3];
        if len_l_raw.fract() != 0.0 || len_r_raw.fract() != 0.0 {
            return Err(CategoryStatus::BadIndex);
        }
        // Both paths carry at least one morphism: identities are
        // explicit carrier morphisms, never an implicit empty path.
        if len_l_raw < 1.0 || len_r_raw < 1.0 {
            return Err(CategoryStatus::BadShape);
        }
        let len_l = len_l_raw as usize;
        let len_r = len_r_raw as usize;
        if cursor + 4 + len_l + len_r > faces.len() {
            return Err(CategoryStatus::BadShape);
        }
        let left = &faces[cursor + 4..cursor + 4 + len_l];
        let right = &faces[cursor + 4 + len_l..cursor + 4 + len_l + len_r];
        let composite = |path: &[f64]| -> Result<usize, CategoryStatus> {
            let mut current = category_index(path[0], k).ok_or(CategoryStatus::BadIndex)?;
            for value in &path[1..] {
                let next = category_index(*value, k).ok_or(CategoryStatus::BadIndex)?;
                let entry = category.table[current][next];
                if entry < 0 {
                    return Err(CategoryStatus::EntryLaw);
                }
                current = entry as usize;
            }
            Ok(current)
        };
        let left_composite = composite(left)?;
        let right_composite = composite(right)?;
        // Path geometry: both paths must run the face's start→end.
        if category.dom[left_composite] != start
            || category.cod[left_composite] != end
            || category.dom[right_composite] != start
            || category.cod[right_composite] != end
        {
            return Err(CategoryStatus::BadShape);
        }
        mask.push(left_composite == right_composite);
        cursor += 4 + len_l + len_r;
    }
    Ok(mask)
}

/// Total commutativity view: the per-face mask (1.0/0.0 in face
/// order), or EMPTY on any refusal (the lp/graph empty-vector
/// convention; the reference interpreter surfaces the typed codes).
pub fn category_diagram_commutative(
    dom: &[f64],
    cod: &[f64],
    comp: &[Vec<f64>],
    faces: &[f64],
) -> Vec<f64> {
    match category_diagram_commutative_status(dom, cod, comp, faces) {
        Ok(mask) => mask
            .iter()
            .map(|face| if *face { 1.0 } else { 0.0 })
            .collect(),
        Err(_) => Vec::new(),
    }
}

pub mod special {
//! `core::special_functions` (05 section 3.3 #1, Phase 11) — contracts
//! and strict-f64 reference implementations.
//!
//! Every result carries an EXPLICIT numeric error bound; nothing here
//! claims correctly-rounded output where that is not proven. Principal
//! branches are named (`W₀`), carriers are declared (real slices), and
//! poles/branch-cut exits refuse with named reasons — never an implicit
//! continuation. Std only, `forbid(unsafe_code)` honored.
//!
//! The [`SpecialFunctionEvaluator`] trait is the provider seam: the
//! strict-f64 reference impls live here; high-precision and
//! interval-certified backends implement the same contract without
//! becoming core semantics.

/// The special functions with contracts in this module.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpecialFn {
    /// Γ(z) — gamma; poles at 0, −1, −2, …
    Gamma,
    /// B(a, b) — beta; positive-real slice a > 0, b > 0.
    Beta,
    /// erf(x) — error function; entire, real line.
    Erf,
    /// ζ(s) — Riemann zeta; simple pole at s = 1; reference carrier is
    /// real s > 1 (the η-series branch).
    Zeta,
    /// W₀(z) — Lambert W, PRINCIPAL branch; real carrier z ≥ −1/e.
    LambertW0,
    /// K(m) — complete elliptic integral of the first kind (parameter
    /// convention); carrier m ∈ [0, 1); K(1) diverges.
    EllipticK,
    /// E(m) — complete elliptic integral of the second kind (parameter
    /// convention); carrier m ∈ [0, 1).
    EllipticE,
    /// Π(n, m) — complete elliptic integral of the third kind.
    /// Contract-only: no reference impl yet (see the no-claim section
    /// of the contract cell).
    EllipticPi,
}

/// Why an evaluation refused. Named, never silent: a pole is not a
/// large number, and a branch exit is not a continuation.
#[derive(Clone, Debug, PartialEq)]
pub enum DomainRefusal {
    /// The argument hits a pole of the function.
    Pole { function: &'static str, at: f64 },
    /// The argument is outside the declared real carrier slice.
    OutsideCarrier {
        function: &'static str,
        carrier: &'static str,
        argument: f64,
    },
    /// Contract exists, reference implementation does not (yet).
    NotImplemented { function: &'static str },
    /// Wrong argument count for the function.
    Arity {
        function: &'static str,
        expected: usize,
        found: usize,
    },
}

/// One evaluation: the value plus the DECLARED error bound. The bound
/// covers the true deviation from the exact special-function value
/// (verified in the contract tests against independently-known
/// references); it is a labeled bound, not a correct-rounding claim.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Evaluated {
    pub value: f64,
    pub error_bound: f64,
}

/// The provider seam (05 §3.3 #1): high-precision / interval-certified
/// backends implement this; core semantics never bake a backend in.
pub trait SpecialFunctionEvaluator {
    fn evaluate(&self, function: SpecialFn, args: &[f64]) -> Result<Evaluated, DomainRefusal>;
}

/// The strict-f64 reference implementation: std-only series /
/// continued-fraction / AGM evaluation with certified error bounds.
pub struct StrictF64Reference;

impl SpecialFunctionEvaluator for StrictF64Reference {
    fn evaluate(&self, function: SpecialFn, args: &[f64]) -> Result<Evaluated, DomainRefusal> {
        let name = function_name(function);
        let expected = match function {
            SpecialFn::Beta => 2,
            SpecialFn::EllipticPi => 3,
            _ => 1,
        };
        if args.len() != expected {
            return Err(DomainRefusal::Arity {
                function: name,
                expected,
                found: args.len(),
            });
        }
        match function {
            SpecialFn::Gamma => gamma_eval(args[0]),
            SpecialFn::Beta => beta_eval(args[0], args[1]),
            SpecialFn::Erf => erf_eval(args[0]),
            SpecialFn::Zeta => zeta_eval(args[0]),
            SpecialFn::LambertW0 => lambert_w0_eval(args[0]),
            SpecialFn::EllipticK => elliptic_k_eval(args[0]),
            SpecialFn::EllipticE => elliptic_e_eval(args[0]),
            SpecialFn::EllipticPi => Err(DomainRefusal::NotImplemented { function: name }),
        }
    }
}

/// Evaluate through the strict-f64 reference without requiring callers
/// to import the provider trait. Generated Rust artifacts use this
/// entry point from their embedded evaluator module.
pub fn evaluate_strict(function: SpecialFn, args: &[f64]) -> Result<Evaluated, DomainRefusal> {
    StrictF64Reference.evaluate(function, args)
}

fn function_name(function: SpecialFn) -> &'static str {
    match function {
        SpecialFn::Gamma => "gamma",
        SpecialFn::Beta => "beta",
        SpecialFn::Erf => "erf",
        SpecialFn::Zeta => "zeta",
        SpecialFn::LambertW0 => "lambert_w0",
        SpecialFn::EllipticK => "elliptic_k",
        SpecialFn::EllipticE => "elliptic_e",
        SpecialFn::EllipticPi => "elliptic_pi",
    }
}

// ---- Γ (Stirling + upward recurrence) --------------------------------

/// Γ(z) via the recurrence to `w = z + n ≥ 12` and the asymptotic
/// Stirling expansion there. The Bernoulli series lives in the
/// EXPONENT (`log Γ = (w−1/2)ln w − w + ½ln 2π + Σ B_{2k}/(2k(2k−1)
/// w^{2k−1}}`); truncating that sum after the B14 term contributes
/// ≤1.9e-21 relative error at w ≥ 12 (terms strictly decreasing
/// there), so `exp` of the truncated sum is the certified correction —
/// NOT a multiplicative first-order stand-in. The declared bound also
/// covers the ≤12 recurrence divisions and exp/pow roundoff
/// (~3e-15 relative total); 1e-14 is declared with margin.
fn stirling_gamma(w: f64) -> f64 {
    let log_correction = 1.0 / (12.0 * w) - 1.0 / (360.0 * w.powi(3)) + 1.0 / (1260.0 * w.powi(5))
        - 1.0 / (1680.0 * w.powi(7))
        + 1.0 / (1188.0 * w.powi(9))
        - 691.0 / (360_360.0 * w.powi(11))
        + 1.0 / (156.0 * w.powi(13));
    (2.0 * std::f64::consts::PI).sqrt() * w.powf(w - 0.5) * (-w).exp() * log_correction.exp()
}

fn gamma_eval(z: f64) -> Result<Evaluated, DomainRefusal> {
    if z <= 0.0 && z.fract() == 0.0 {
        return Err(DomainRefusal::Pole {
            function: "gamma",
            at: z,
        });
    }
    if !z.is_finite() {
        return Err(DomainRefusal::OutsideCarrier {
            function: "gamma",
            carrier: "finite real z; poles at 0, −1, −2, …",
            argument: z,
        });
    }
    if z < 0.5 {
        // Reflection: Γ(z) = π / (sin(πz) Γ(1−z)).
        let mirror = gamma_direct(1.0 - z);
        let sin_pi_z = (std::f64::consts::PI * z).sin();
        // sin(πz) ≈ 0 near negative integers: the value explodes; the
        // declared carrier for the reflection branch stops at |sin| ≥
        // 1e-3 (between-pole points stay admitted, bound inflated).
        if sin_pi_z.abs() < 1e-3 {
            return Err(DomainRefusal::OutsideCarrier {
                function: "gamma",
                carrier: "reflection branch needs |sin(πz)| ≥ 1e-3 (near-pole points refuse)",
                argument: z,
            });
        }
        let value = std::f64::consts::PI / (sin_pi_z * mirror);
        // Bound: direct-branch relative bound (1e-14) plus reflection
        // conditioning (1/|sin| amplification).
        let bound = value.abs() * 1e-14 * (1.0 / sin_pi_z.abs());
        return Ok(Evaluated {
            value,
            error_bound: bound,
        });
    }
    let value = gamma_direct(z);
    Ok(Evaluated {
        value,
        error_bound: value.abs() * 1e-14,
    })
}

fn gamma_direct(z: f64) -> f64 {
    // Shift up to w ≥ 12: Γ(z) = Γ(w) / (z·(z+1)···(w−1)).
    let mut w = z;
    let mut product = 1.0_f64;
    while w < 12.0 {
        product *= w;
        w += 1.0;
    }
    stirling_gamma(w) / product
}

// ---- B(a, b) ---------------------------------------------------------

fn beta_eval(a: f64, b: f64) -> Result<Evaluated, DomainRefusal> {
    if a <= 0.0 || b <= 0.0 {
        return Err(DomainRefusal::OutsideCarrier {
            function: "beta",
            carrier: "positive-real slice a > 0, b > 0",
            argument: if a <= 0.0 { a } else { b },
        });
    }
    let ga = gamma_eval(a)?;
    let gb = gamma_eval(b)?;
    let gab = gamma_eval(a + b)?;
    let value = ga.value * gb.value / gab.value;
    // First-order composition of the relative bounds.
    let bound = value.abs()
        * (ga.error_bound / ga.value.abs()
            + gb.error_bound / gb.value.abs()
            + gab.error_bound / gab.value.abs());
    Ok(Evaluated {
        value,
        error_bound: bound,
    })
}

// ---- erf -------------------------------------------------------------

fn erf_eval(x: f64) -> Result<Evaluated, DomainRefusal> {
    if !x.is_finite() {
        return Err(DomainRefusal::OutsideCarrier {
            function: "erf",
            carrier: "finite real x (erf is entire)",
            argument: x,
        });
    }
    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let ax = x.abs();
    if ax >= 4.0 {
        // Tail certificate: 1 − erf(4) ≈ 1.54e-8 and decreasing; the
        // constant 1 with that bound is the declared value here.
        return Ok(Evaluated {
            value: sign as f64,
            error_bound: 1.55e-8,
        });
    }
    // Maclaurin series, terms via the ratio t_{n+1} = t_n·(−x²(2n+1))/(n+1)(2n+3):
    // t_n = (−1)^n x^{2n+1}/(n!(2n+1)). Alternating with eventually
    // decreasing magnitude on |x| ≤ 4; the alternating-tail bound is
    // certified once terms decrease (tracked explicitly).
    let inv_sqrt_pi = 2.0 / std::f64::consts::PI.sqrt();
    let mut term = ax; // t_0
    let mut sum = ax;
    let mut n = 0.0_f64;
    let mut decreasing = false;
    let mut next_term;
    loop {
        next_term = -term * (ax * ax) * (2.0 * n + 1.0) / ((n + 1.0) * (2.0 * n + 3.0));
        n += 1.0;
        if next_term.abs() <= term.abs() {
            decreasing = true;
        }
        term = next_term;
        sum += term;
        if decreasing && term.abs() < 1e-18 {
            break;
        }
        if n > 400.0 {
            break;
        }
    }
    let bound = inv_sqrt_pi * term.abs();
    // Roundoff honesty: ~N additions each with ≤½ulp relative error
    // make the accumulated error up to ~Σ|terms|·1e-16 — for large
    // args the alternating remainder alone understates it. Declare the
    // larger of the two.
    let roundoff = inv_sqrt_pi * sum.abs() * (n * 1e-16);
    Ok(Evaluated {
        value: sign as f64 * inv_sqrt_pi * sum,
        error_bound: bound.max(roundoff),
    })
}

// ---- ζ(s) ------------------------------------------------------------

fn zeta_eval(s: f64) -> Result<Evaluated, DomainRefusal> {
    if !s.is_finite() {
        return Err(DomainRefusal::OutsideCarrier {
            function: "zeta",
            carrier: "real s > 1",
            argument: s,
        });
    }
    if s == 1.0 {
        return Err(DomainRefusal::Pole {
            function: "zeta",
            at: s,
        });
    }
    if s <= 1.0 {
        return Err(DomainRefusal::OutsideCarrier {
            function: "zeta",
            carrier: "real s > 1 (η-series reference branch)",
            argument: s,
        });
    }
    // ζ(s) = η(s)/(1 − 2^{1−s}); η alternating with |R_N| ≤ (N+1)^{−s}.
    let divisor = 1.0 - (1.0 - s).exp2();
    let mut n = 1.0_f64;
    let mut eta = 0.0_f64;
    let mut tail_bound;
    loop {
        let term = 1.0 / n.powf(s);
        eta += if ((n as u64) % 2) == 1 { term } else { -term };
        tail_bound = (n + 1.0).powf(-s) / divisor.abs();
        if tail_bound < 1e-14 * eta.abs().max(1e-300) || n > 8.0e6 {
            break;
        }
        n += 1.0;
    }
    let value = eta / divisor;
    // Roundoff honesty: the η partial sum has up to ~n·½ulp relative
    // error, amplified through the (fixed) divisor; the declared bound
    // is the larger of the alternating tail and the roundoff.
    let roundoff = eta.abs() * (n * 1e-16) / divisor.abs();
    Ok(Evaluated {
        value,
        error_bound: tail_bound.max(roundoff),
    })
}

// ---- W₀(z) -----------------------------------------------------------

fn lambert_w0_eval(z: f64) -> Result<Evaluated, DomainRefusal> {
    if !z.is_finite() {
        return Err(DomainRefusal::OutsideCarrier {
            function: "lambert_w0",
            carrier: "real z ≥ −1/e (principal branch)",
            argument: z,
        });
    }
    let branch_point = -std::f64::consts::E.recip();
    if z < branch_point {
        return Err(DomainRefusal::OutsideCarrier {
            function: "lambert_w0",
            carrier: "real z ≥ −1/e (principal branch W₀; branch cut at (−∞, −1/e))",
            argument: z,
        });
    }
    if z == branch_point {
        return Ok(Evaluated {
            value: -1.0,
            error_bound: 0.0,
        });
    }
    if z == 0.0 {
        return Ok(Evaluated {
            value: 0.0,
            error_bound: 0.0,
        });
    }
    // Initial guess: series near 0, log form for larger arguments
    // (`ln(1+z)` is finite and positive for z > −1/e + …, never the
    // ln(1) = 0 → ln ln z = −∞ blowup of the naive asymptotic form).
    let mut w = if z.abs() < 1.0 {
        let z2 = z * z;
        z - z2 + 1.5 * z2 * z - 8.0 / 3.0 * z2 * z2
    } else {
        (1.0 + z).ln()
    };
    // Halley iterations (cubic convergence), to fixed point.
    for _ in 0..100 {
        let e_w = w.exp();
        let p = w * e_w;
        let delta = p - z;
        let numerator = delta / (e_w * (w + 1.0) - (w + 2.0) * delta / (2.0 * w + 2.0));
        let next = w - numerator;
        if (next - w).abs() <= 1e-16 * (1.0 + next.abs()) {
            w = next;
            break;
        }
        w = next;
    }
    // Residual certificate: |w − W(z)| ≈ |w e^w − z| / (e^w·|1+w|).
    let residual = (w * w.exp() - z).abs();
    let derivative = w.exp() * (1.0 + w).abs();
    let bound = if derivative > 0.0 {
        residual / derivative
    } else {
        residual
    };
    Ok(Evaluated {
        value: w,
        error_bound: bound,
    })
}

// ---- K(m), E(m) ------------------------------------------------------

fn elliptic_domain_check(m: f64) -> Result<(), DomainRefusal> {
    if !m.is_finite() || !(0.0..1.0).contains(&m) {
        return Err(DomainRefusal::OutsideCarrier {
            function: "elliptic",
            carrier: "parameter m ∈ [0, 1) (K(1) diverges)",
            argument: m,
        });
    }
    Ok(())
}

/// AGM evaluation of K(m) = π/(2·AGM(1, √(1−m))).
/// Certificate: b_N ≤ a_∞ ≤ a_N, so |Δa| ≤ a_N − b_N, propagated
/// through K = π/(2a) as a relative bound.
fn elliptic_k_eval(m: f64) -> Result<Evaluated, DomainRefusal> {
    elliptic_domain_check(m)?;
    let mut a = 1.0_f64;
    let mut b = (1.0 - m).sqrt();
    for _ in 0..80 {
        let next_a = (a + b) / 2.0;
        b = (a * b).sqrt();
        a = next_a;
        if a - b <= 1e-16 * a {
            break;
        }
    }
    let value = std::f64::consts::PI / (2.0 * a);
    let bound = value * (a - b) / a;
    Ok(Evaluated {
        value,
        error_bound: bound,
    })
}

/// E(m) = (π/2)·₂F₁(−1/2, 1/2; 1; m) by the hypergeometric series.
/// Certificate: all terms after t₀ are negative; the tail is bounded
/// with the exact ratio |t_{n+1}| ≤ r·|t_n| at the stopping index:
/// tail ≤ |t_{N+1}|·(N+1)²/(2N+5/4).
fn elliptic_e_eval(m: f64) -> Result<Evaluated, DomainRefusal> {
    elliptic_domain_check(m)?;
    let mut term = 1.0_f64; // t_0 = 1
    let mut sum = 1.0_f64;
    let mut n = 0.0_f64;
    let mut next;
    loop {
        // t_{n+1}/t_n = ((n−1/2)(n+1/2)/(n+1)²)·m — the hypergeometric
        // ratio in the ARGUMENT m (dropping it made E(0) ≠ π/2).
        next = term * ((n - 0.5) * (n + 0.5)) / ((n + 1.0) * (n + 1.0)) * m;
        n += 1.0;
        term = next;
        sum += next;
        if next.abs() * (n + 1.0) * (n + 1.0) / (2.0 * n + 1.25) < 1e-16 || n > 1.0e7 {
            break;
        }
    }
    let value = std::f64::consts::PI / 2.0 * sum;
    // Tail certificate recomputed at the stopping index (all-same-sign
    // series: partial sums approach from one side), plus roundoff.
    let tail = next.abs() * (n + 1.0) * (n + 1.0) / (2.0 * n + 1.25);
    let roundoff = sum.abs() * (n * 1e-16);
    let bound = std::f64::consts::PI / 2.0 * (tail.max(roundoff));
    Ok(Evaluated {
        value,
        error_bound: bound,
    })
}

}

}
/// `genfun_cauchy`: a `function` declaration generated from `.emath`.
/// Generated deterministically by emath Phase 1; do not edit.
/// Evaluate `cauchy_ok` (strict-f64, Phase 1). Index/slice out of bounds is `Err`.
pub fn genfun_cauchy(n: i64) -> Result<bool, String> {
    {
        let g = {
            emath_rt::sequence_generate(&(vec!((0.0), (1.0))), &(vec!((1.0), (1.0))), &(16.0))
        };
        let square = {
            emath_rt::sequence_convolve(&(g), &(g), &(16.0))
        };
        let d2 = {
            let __e2 = emath_rt::vec_index_checked(&(g), (0.0)).map_err(|e| e.to_string())?;
            let __e5 = emath_rt::vec_index_checked(&(g), (2.0)).map_err(|e| e.to_string())?;
            let __e9 = emath_rt::vec_index_checked(&(g), (1.0)).map_err(|e| e.to_string())?;
            let __e12 = emath_rt::vec_index_checked(&(g), (1.0)).map_err(|e| e.to_string())?;
            let __e17 = emath_rt::vec_index_checked(&(g), (2.0)).map_err(|e| e.to_string())?;
            let __e20 = emath_rt::vec_index_checked(&(g), (0.0)).map_err(|e| e.to_string())?;
            ((__e2 * __e5) + (__e9 * __e12)) + (__e17 * __e20)
        };
        let d3 = {
            let __e2 = emath_rt::vec_index_checked(&(g), (0.0)).map_err(|e| e.to_string())?;
            let __e5 = emath_rt::vec_index_checked(&(g), (3.0)).map_err(|e| e.to_string())?;
            let __e9 = emath_rt::vec_index_checked(&(g), (1.0)).map_err(|e| e.to_string())?;
            let __e12 = emath_rt::vec_index_checked(&(g), (2.0)).map_err(|e| e.to_string())?;
            let __e17 = emath_rt::vec_index_checked(&(g), (2.0)).map_err(|e| e.to_string())?;
            let __e20 = emath_rt::vec_index_checked(&(g), (1.0)).map_err(|e| e.to_string())?;
            let __e25 = emath_rt::vec_index_checked(&(g), (3.0)).map_err(|e| e.to_string())?;
            let __e28 = emath_rt::vec_index_checked(&(g), (0.0)).map_err(|e| e.to_string())?;
            (((__e2 * __e5) + (__e9 * __e12)) + (__e17 * __e20)) + (__e25 * __e28)
        };
        Ok({
    let __e2 = emath_rt::vec_index_checked(&(square), ((2i64)) as f64).map_err(|e| e.to_string())?;
    let __e7 = emath_rt::vec_index_checked(&(square), ((3i64)) as f64).map_err(|e| e.to_string())?;
    let __e13 = emath_rt::vec_index_checked(&(square), ((0i64)) as f64).map_err(|e| e.to_string())?;
    let __e19 = emath_rt::vec_index_checked(&(square), ((1i64)) as f64).map_err(|e| e.to_string())?;
    (((__e2 == (d2)) && (__e7 == (d3))) && (__e13 == (0.0))) && (__e19 == (0.0))
}
)
    }
}

/// Example test: `fib`.
#[allow(clippy::float_cmp)]
#[test]
fn genfun_cauchy_fib() {
    {
        let n = 3i64;
        let actual = genfun_cauchy(n)
            .expect("index in bounds");
        let cauchy_ok = actual;
        assert!({
            (cauchy_ok) == (true)
        });
    }
}

/// `genfun_recurrence`: a `function` declaration generated from `.emath`.
/// Generated deterministically by emath Phase 1; do not edit.
/// Evaluate `rec_ok` (strict-f64, Phase 1). Index/slice out of bounds is `Err`.
pub fn genfun_recurrence(n: i64) -> Result<bool, String> {
    {
        let g = {
            emath_rt::sequence_generate(&(vec!((0.0), (1.0))), &(vec!((1.0), (1.0))), &(16.0))
        };
        let fib7 = {
            let __e2 = emath_rt::vec_index_checked(&(g), (7.0)).map_err(|e| e.to_string())?;
            __e2
        };
        Ok({
    let __e5 = emath_rt::vec_index_checked(&(g), (8.0)).map_err(|e| e.to_string())?;
    let __e8 = emath_rt::vec_index_checked(&(g), (7.0)).map_err(|e| e.to_string())?;
    let __e12 = emath_rt::vec_index_checked(&(g), (6.0)).map_err(|e| e.to_string())?;
    let __e19 = emath_rt::vec_index_checked(&(g), (2.0)).map_err(|e| e.to_string())?;
    let __e22 = emath_rt::vec_index_checked(&(g), (1.0)).map_err(|e| e.to_string())?;
    let __e26 = emath_rt::vec_index_checked(&(g), (0.0)).map_err(|e| e.to_string())?;
    (((fib7) == (13.0)) && (((__e5 - __e8) - __e12) == (0.0))) && (((__e19 - __e22) - __e26) == (0.0))
}
)
    }
}

/// Example test: `fib`.
#[allow(clippy::float_cmp)]
#[test]
fn genfun_recurrence_fib() {
    {
        let n = 8i64;
        let actual = genfun_recurrence(n)
            .expect("index in bounds");
        let rec_ok = actual;
        assert!({
            (rec_ok) == (true)
        });
    }
}
