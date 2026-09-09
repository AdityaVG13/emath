// Pre-compiled math kernels, embedded verbatim into generated crates as
// `mod emath_rt { ... }`. Keep this file std-only (no external crates, no
// `crate::` paths, no crate attributes) and deterministic: same inputs,
// same IEEE-754 operation order, bit-for-bit same output.

// ── Complex carriers (VM builtin backstops) ──────────────────────────────────

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

// ── Matrices (row-major nested rows) ──────────────────────────────────────

// ── Tensors (flat storage) ────────────────────────────────────────────────

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
pub fn vec_index_checked<T: Clone>(v: &[T], index: f64) -> Result<T, IndexError> {
    let i = whole_index(index, v.len())?;
    Ok(v[i].clone())
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

