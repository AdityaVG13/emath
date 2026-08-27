//! Pre-compiled math kernels, embedded verbatim into generated crates as
//! `mod emath_rt { ... }`. Keep this file std-only (no external crates, no
//! `crate::` paths, no crate attributes) and deterministic: same inputs,
//! same IEEE-754 operation order, bit-for-bit same output.

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
        result = (result * x + exact_i64(c)?).rem_euclid(p);
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
            result = (result * x + exact_i64(c)?).rem_euclid(p);
        }
        codeword.push(result as f64);
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
