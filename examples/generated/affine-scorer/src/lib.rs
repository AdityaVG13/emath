#![forbid(unsafe_code)]
#![allow(dead_code)]
#[allow(dead_code)]
mod emath_rt {
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


// ── Richer linear algebra (xx0x.2) ────────────────────────────────────────
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
pub fn eig_symmetric(
    flat: &[f64],
    rows: usize,
    cols: usize,
) -> (Vec<f64>, Vec<Vec<f64>>) {
    if rows != cols || rows == 0 {
        return (Vec::new(), Vec::new());
    }
    let n = rows;
    let mut work: Vec<Vec<f64>> = (0..n)
        .map(|r| flat[r * cols..r * cols + cols].to_vec())
        .collect();
    // Symmetry gate (relative tolerance; rounding noise admits).
    let magnitude: f64 = work.iter().flat_map(|row| row.iter()).map(|x| x.abs()).sum();
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
                let t =
                    theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
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
            ata[i][j] = (0..rows).map(|k| flat[k * cols + i] * flat[k * cols + j]).sum();
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
            let dot: f64 = (0..cols)
                .map(|i| flat[row * cols + i] * v_rows[k][i])
                .sum();
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

// ── Graph traversal (r2-graphs-masa slice 1) ──────────────────────────────
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
            .min_by(|x, y| {
                distances[*x]
                    .total_cmp(&distances[*y])
                    .then(x.cmp(y))
            })
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

// ── Nested-carrier adapters (xx0x.2 spectral/iterative kernels) ───────────
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

/// Unnormalized graph Laplacian over a DENSE adjacency carrier:
/// `L = D − A` where `D` is the out-degree diagonal (nonzero-entry
/// counts, slice 1's degree law) and `A` is the carrier. The Laplacian
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

// ── Linear programming + multi-objective (r3-lp-milp-wlif slice 1) ────────
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

// ── Polynomials as values (r3-funcspaces-poly-hjor slice 1) ───────────────
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

// ── Stiff + symplectic ODE nucleus (xx0x.3 thin slice) ────────────────────
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
pub fn ode_backward_euler_step(
    rate_coefficients: &[f64],
    y0: f64,
    h: f64,
) -> Vec<f64> {
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

// ── Spectral Poisson, 1D Dirichlet (xx0x.4 thin slice) ────────────────────
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
                    f * (std::f64::consts::PI * (j as f64 + 1.0) * k as f64 / (n_f + 1.0))
                        .sin()
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
                        * (std::f64::consts::PI * j as f64 * (k as f64 + 1.0) / (n_f + 1.0))
                            .sin()
                })
                .sum::<f64>()
                * (2.0 / (n_f + 1.0))
        })
        .collect()
}

// ── Probability: seeded sampling + densities (xx0x.5 thin slice) ─────────
//
// ONE generator, one place: SplitMix64 (the compute-layer nucleus the
// vnqo stream contract composes above — no second RNG namespace).
// The seed is an f64 scalar whose to_bits() initializes the state
// (PROVISIONAL mapping; re-mappable by the vnqo contract without
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
pub fn prob_sample(
    kind: u8,
    params: &[f64],
    seed: f64,
    draws: usize,
) -> Vec<f64> {
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
            let normalization =
                1.0 / (sigma * (2.0 * std::f64::consts::PI).sqrt());
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

// ── Graph symmetrization (masa slice 4: directed → spectral path) ────────
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
                .map(|i| {
                    (0..n)
                        .map(|j| (adj[i][j] + adj[j][i]) / 2.0)
                        .collect()
                })
                .collect()
        }
        _ => Vec::new(),
    }
}

// ── Bellman-Ford: negative-edge shortest paths (masa slice 5) ────────────
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

// ── Sparse storage: COO triplet carrier (masa slice 6) ───────────────────
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
        let (Some(u), Some(v)) =
            (graph_source_index(*u, n), graph_source_index(*v, n))
        else {
            return Vec::new();
        };
        if !weight.is_finite() {
            return Vec::new();
        }
        adj[u][v] += weight;
    }
    adj
}

}
/// `AffineScorer`: a `policy` declaration generated from `.emath`.
/// Generated deterministically by emath Phase 1; do not edit.
#[derive(Clone, Debug)]
pub struct AffineScorer {
    scale: f64,
    bias: f64,
}

/// Configuration error type returned by failed constructors.
#[derive(Clone, Debug, PartialEq)]
pub enum ConfigError {
    /// A constructor `require` invariant did not hold.
    FailedPrecondition,
    /// A constructor `ensure`/`invariant` did not hold after field init.
    FailedPostcondition,
}

impl AffineScorer {
    /// Construct an `AffineScorer`; every `require` and `ensure` invariant is checked.
    pub fn new(scale: f64, bias: f64) -> Result<Self, ConfigError> {
        {
            let __ok0 = !{
                matches!(emath_rt::cmp_i64_f64((0i64), (scale)), Some(core::cmp::Ordering::Less | core::cmp::Ordering::Equal))
            };
            if __ok0 {
                {
                    return Err(ConfigError::FailedPrecondition);
                }
            }
            let __ok1 = !{
                (scale).is_finite()
            };
            if __ok1 {
                {
                    return Err(ConfigError::FailedPrecondition);
                }
            }
            let __ok2 = !{
                (bias).is_finite()
            };
            if __ok2 {
                {
                    return Err(ConfigError::FailedPrecondition);
                }
            }
            let __post_ok0 = !{
                emath_rt::cmp_i64_f64((0i64), (scale)) == Some(core::cmp::Ordering::Less)
            };
            if __post_ok0 {
                {
                    return Err(ConfigError::FailedPostcondition);
                }
            }
            Ok(Self { scale, bias })
        }
    }
    /// Evaluate `score` (strict-f64, Phase 1).
    pub fn score(&self, x: f64) -> f64 {
        {
            {
                ((self.scale) * (x)) + (self.bias)
            }
        }
    }
}

/// Example test: `score_is_seven`.
#[allow(clippy::float_cmp)]
#[test]
fn affine_scorer_score_is_seven() {
    {
        let bias = 4.0;
        let scale = 1.0;
        let x = 3.0;
        let affine_scorer = AffineScorer::new(scale, bias)
            .expect("constructor invariants must hold for this example");
        let actual = affine_scorer.score(x);
        let score = actual;
        assert!({
            (score) == (7.0)
        });
    }
}

/// Example test: `fractional_score`.
#[allow(clippy::float_cmp)]
#[test]
fn affine_scorer_fractional_score() {
    {
        let bias = 0.5;
        let scale = 2.0;
        let x = 1.5;
        let affine_scorer = AffineScorer::new(scale, bias)
            .expect("constructor invariants must hold for this example");
        let actual = affine_scorer.score(x);
        let score = actual;
        assert!({
            (score) == (3.5)
        });
    }
}
