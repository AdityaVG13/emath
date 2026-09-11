#![forbid(unsafe_code)]
#![allow(dead_code)]
#[allow(dead_code)]
pub mod emath_rt {
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

/// Complex scalar arithmetic, codegen parity twins of the interpreter's
/// complex arm in the scalar apply (`exec-ir interp.rs`): same formulas,
/// same IEEE-754 operation order, bit-for-bit same output.
pub fn complex_add(a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
    (a.0 + b.0, a.1 + b.1)
}

pub fn complex_sub(a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
    (a.0 - b.0, a.1 - b.1)
}

pub fn complex_mul(a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
    (a.0 * b.0 - a.1 * b.1, a.0 * b.1 + a.1 * b.0)
}

pub fn complex_div(a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
    let denominator = b.0 * b.0 + b.1 * b.1;
    (
        (a.0 * b.0 + a.1 * b.1) / denominator,
        (a.1 * b.0 - a.0 * b.1) / denominator,
    )
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
// Nested-carrier outer product over dense vectors.

/// Outer product `a bᵀ`.
pub fn outer_product(a: &[f64], b: &[f64]) -> Vec<Vec<f64>> {
    if a.is_empty() || b.is_empty() || a.iter().chain(b).any(|value| !value.is_finite()) {
        return Vec::new();
    }
    a.iter()
        .map(|left| b.iter().map(|right| left * right).collect())
        .collect()
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
// Exact kernels use the existing (numerator, denominator) representation.
// Every public result has a positive, gcd-reduced denominator. Checked i128
// intermediates can refuse; no operation converts an exact value to a float.
pub type ExactRatio = (i128, i128);
type ExactResult<T> = Result<T, String>;
const QZERO: ExactRatio = (0, 1);
const QONE: ExactRatio = (1, 1);
fn q_overflow() -> String {
    "E-RAT-002: exact rational intermediate exceeds i128".into()
}
fn q_gcd(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}
pub fn ratio_normalize(n: i128, d: i128) -> ExactResult<ExactRatio> {
    if d == 0 {
        return Err("E-RAT-001: denominator is zero".into());
    }
    let g = q_gcd(n.unsigned_abs(), d.unsigned_abs());
    let magnitude = n.unsigned_abs() / g;
    let denominator = i128::try_from(d.unsigned_abs() / g).map_err(|_| q_overflow())?;
    let negative = (n < 0) != (d < 0);
    let numerator = if negative && magnitude == (1_u128 << 127) {
        i128::MIN
    } else {
        let value = i128::try_from(magnitude).map_err(|_| q_overflow())?;
        if negative { -value } else { value }
    };
    Ok((numerator, denominator))
}
pub fn ratio_construct(n: i64, d: i64) -> ExactResult<ExactRatio> {
    ratio_normalize(i128::from(n), i128::from(d))
}
pub fn ratio_norm(q: ExactRatio) -> ExactResult<ExactRatio> {
    ratio_normalize(q.0, q.1)
}
fn q_valid(q: ExactRatio) -> bool {
    ratio_norm(q).is_ok_and(|canonical| canonical == q)
}
fn q_require(q: ExactRatio) -> ExactResult<()> {
    if q_valid(q) {
        Ok(())
    } else {
        Err("E-RAT-001: noncanonical rational".into())
    }
}
pub fn ratio_add(a: ExactRatio, b: ExactRatio) -> ExactResult<ExactRatio> {
    q_sum(a, b, false)
}
pub fn ratio_sub(a: ExactRatio, b: ExactRatio) -> ExactResult<ExactRatio> {
    q_sum(a, b, true)
}
fn q_sum(a: ExactRatio, b: ExactRatio, subtract: bool) -> ExactResult<ExactRatio> {
    q_require(a)?;
    q_require(b)?;
    let g = q_gcd(a.1 as u128, b.1 as u128) as i128;
    let left = a.0.checked_mul(b.1 / g).ok_or_else(q_overflow)?;
    let right = b.0.checked_mul(a.1 / g).ok_or_else(q_overflow)?;
    let n = if subtract {
        left.checked_sub(right)
    } else {
        left.checked_add(right)
    }
    .ok_or_else(q_overflow)?;
    let h = q_gcd(n.unsigned_abs(), g as u128) as i128;
    ratio_normalize(
        n / h,
        (a.1 / g).checked_mul(b.1 / h).ok_or_else(q_overflow)?,
    )
}
pub fn ratio_mul(a: ExactRatio, b: ExactRatio) -> ExactResult<ExactRatio> {
    q_require(a)?;
    q_require(b)?;
    let g = q_gcd(a.0.unsigned_abs(), b.1 as u128) as i128;
    let h = q_gcd(b.0.unsigned_abs(), a.1 as u128) as i128;
    ratio_normalize(
        (a.0 / g).checked_mul(b.0 / h).ok_or_else(q_overflow)?,
        (a.1 / h).checked_mul(b.1 / g).ok_or_else(q_overflow)?,
    )
}
pub fn ratio_div(a: ExactRatio, b: ExactRatio) -> ExactResult<ExactRatio> {
    q_require(a)?;
    q_require(b)?;
    if b.0 == 0 {
        return Err("E-RAT-001: division by zero".into());
    }
    // Reduce unsigned numerator magnitudes before forming the reciprocal. This
    // permits MIN/MIN even though the positive reciprocal denominator cannot fit.
    let g = q_gcd(a.0.unsigned_abs(), b.0.unsigned_abs());
    let h = q_gcd(a.1 as u128, b.1 as u128) as i128;
    let divide_magnitude = |n: i128| -> ExactResult<i128> {
        let magnitude = n.unsigned_abs() / g;
        if n < 0 && magnitude == (1_u128 << 127) {
            Ok(i128::MIN)
        } else {
            let v = i128::try_from(magnitude).map_err(|_| q_overflow())?;
            Ok(if n < 0 { -v } else { v })
        }
    };
    ratio_normalize(
        divide_magnitude(a.0)?
            .checked_mul(b.1 / h)
            .ok_or_else(q_overflow)?,
        (a.1 / h)
            .checked_mul(divide_magnitude(b.0)?)
            .ok_or_else(q_overflow)?,
    )
}
fn q_cmp(a: ExactRatio, b: ExactRatio) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    if (a.0 < 0) != (b.0 < 0) {
        return a.0.cmp(&b.0);
    }
    let (mut n, mut d, mut p, mut q) = (
        a.0.unsigned_abs(),
        a.1 as u128,
        b.0.unsigned_abs(),
        b.1 as u128,
    );
    let mut reverse = a.0 < 0;
    loop {
        let order = (n / d).cmp(&(p / q));
        if order != Ordering::Equal {
            return if reverse { order.reverse() } else { order };
        }
        let (r, s) = (n % d, p % q);
        if r == 0 || s == 0 {
            let order = r.cmp(&s);
            return if reverse { order.reverse() } else { order };
        }
        (n, d, p, q) = (d, r, q, s);
        reverse = !reverse;
    }
}
pub fn ratio_lt(a: ExactRatio, b: ExactRatio) -> ExactResult<bool> {
    q_require(a)?;
    q_require(b)?;
    Ok(q_cmp(a, b).is_lt())
}

pub mod special {

}

}
#[derive(Clone, Debug, PartialEq)]
pub struct EmathRecord_AgmState {
    pub a: f64,
    pub b: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmathRecord_BfsQueue {
    pub front: Vec<f64>,
    pub vis: Vec<f64>,
    pub ord: Vec<f64>,
    pub head: i64,
    pub tail: i64,
    pub count: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmathRecord_BfsWork {
    pub vis: Vec<f64>,
    pub front: Vec<f64>,
    pub tail: i64,
    pub ord: Vec<f64>,
    pub count: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmathRecord_CG {
    pub x: Vec<f64>,
    pub r: Vec<f64>,
    pub d: Vec<f64>,
    pub rs: f64,
    pub failed: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmathRecord_CarlsonRfState {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub done: bool,
    pub value: f64,
    pub error_bound: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmathRecord_CarlsonRjState {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub p: f64,
    pub sum: f64,
    pub bound_sum: f64,
    pub factor: f64,
    pub done: bool,
    pub value: f64,
    pub error_bound: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmathRecord_CategoryCarrier {
    pub k: i64,
    pub objects: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmathRecord_CategoryFace {
    pub cursor: i64,
    pub mask: Vec<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmathRecord_CharState {
    pub m: Vec<f64>,
    pub coeffs: Vec<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmathRecord_DistBest {
    pub bi: i64,
    pub bd: f64,
    pub any: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmathRecord_DistState {
    pub dist: Vec<f64>,
    pub settled: Vec<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmathRecord_DynamicsEulerState {
    pub x: f64,
    pub done: bool,
    pub code: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmathRecord_EllipticESeriesState {
    pub term: f64,
    pub sum: f64,
    pub n: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmathRecord_ErfSeriesState {
    pub term: f64,
    pub sum: f64,
    pub n: f64,
    pub decreasing: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmathRecord_Estimate {
    pub value: f64,
    pub method: String,
    pub n: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmathRecord_GameShape {
    pub rows: i64,
    pub cols: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmathRecord_GammaShiftState {
    pub w: f64,
    pub product: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmathRecord_HalleyState {
    pub current: f64,
    pub next: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmathRecord_JState {
    pub a: Vec<f64>,
    pub v: Vec<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmathRecord_KthCell {
    pub u: i64,
    pub v: i64,
    pub w: f64,
    pub seen: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmathRecord_LinearCases {
    pub a: emath_rt::ExactRatio,
    pub b: emath_rt::ExactRatio,
    pub a_known: bool,
    pub b_known: bool,
    pub conditions: Vec<String>,
    pub solutions: Vec<String>,
    pub values: Vec<Vec<emath_rt::ExactRatio>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmathRecord_LinearFamily {
    pub coefficients: Vec<emath_rt::ExactRatio>,
    pub rhs: Vec<emath_rt::ExactRatio>,
    pub columns: i64,
    pub reduced: Vec<Vec<emath_rt::ExactRatio>>,
    pub transformed_rhs: Vec<emath_rt::ExactRatio>,
    pub transform: Vec<Vec<emath_rt::ExactRatio>>,
    pub inverse: Vec<Vec<emath_rt::ExactRatio>>,
    pub pivots: Vec<i64>,
    pub cursor: i64,
    pub complete: bool,
    pub status: String,
    pub particular: Vec<emath_rt::ExactRatio>,
    pub basis: Vec<Vec<emath_rt::ExactRatio>>,
    pub witness: Vec<emath_rt::ExactRatio>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmathRecord_LpDims {
    pub m: i64,
    pub n: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmathRecord_NewtonCheck {
    pub residual: f64,
    pub converged: bool,
    pub h: f64,
    pub y_plus: f64,
    pub budget: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmathRecord_NewtonUpdate {
    pub next: f64,
    pub singular: bool,
    pub nonfinite: bool,
    pub d: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmathRecord_PolynomialBracket {
    pub coefficients: Vec<emath_rt::ExactRatio>,
    pub initial_lower: emath_rt::ExactRatio,
    pub initial_upper: emath_rt::ExactRatio,
    pub lower: emath_rt::ExactRatio,
    pub upper: emath_rt::ExactRatio,
    pub tolerance: emath_rt::ExactRatio,
    pub steps: i64,
    pub complete: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmathRecord_ProgramOptimizeLimit {
    pub prev: f64,
    pub best: f64,
    pub done: bool,
    pub result: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmathRecord_ProgramOptimizePoint {
    pub x: Vec<f64>,
    pub grads: Vec<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmathRecord_ProgramSolveBisect {
    pub left: f64,
    pub right: f64,
    pub left_res: f64,
    pub done: bool,
    pub found: bool,
    pub value: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmathRecord_ProgramSolveBracket {
    pub radius: f64,
    pub done: bool,
    pub found: bool,
    pub value: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmathRecord_ProgramSolveNewton {
    pub x: f64,
    pub done: bool,
    pub code: i64,
    pub value: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmathRecord_QuantizedTensor {
    pub shape: Vec<i64>,
    pub codes: Vec<i64>,
    pub scale: f64,
    pub zero_point: i64,
    pub bits: i64,
    pub signed: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmathRecord_RatioBest {
    pub leave: i64,
    pub best: f64,
    pub has: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmathRecord_ResidualNewtonState {
    pub x: Vec<f64>,
    pub f: Vec<f64>,
    pub fmax: f64,
    pub done: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmathRecord_Rk45Decision {
    pub code: i64,
    pub rel: f64,
    pub next: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmathRecord_Rk45ErrScale {
    pub err: f64,
    pub scale: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmathRecord_RouthRows {
    pub a: Vec<f64>,
    pub b: Vec<f64>,
    pub la: i64,
    pub lb: i64,
    pub prev: bool,
    pub code: i64,
    pub done: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmathRecord_SeriesEnclosure {
    pub first: emath_rt::ExactRatio,
    pub z: emath_rt::ExactRatio,
    pub a: emath_rt::ExactRatio,
    pub b: emath_rt::ExactRatio,
    pub c: emath_rt::ExactRatio,
    pub factorial: bool,
    pub tolerance: emath_rt::ExactRatio,
    pub terms: Vec<emath_rt::ExactRatio>,
    pub partial: emath_rt::ExactRatio,
    pub lower: emath_rt::ExactRatio,
    pub upper: emath_rt::ExactRatio,
    pub complete: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmathRecord_SimplexState {
    pub m: i64,
    pub n: i64,
    pub width: i64,
    pub w1: i64,
    pub tab: Vec<f64>,
    pub basis: Vec<f64>,
    pub cost: Vec<f64>,
    pub status: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmathRecord_SpecialEstimate {
    pub value: f64,
    pub error_bound: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmathRecord_TieScan {
    pub val: i64,
    pub seen: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmathRecord_TriScan {
    pub idx: i64,
    pub code: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmathRecord_VerletHalf {
    pub half: f64,
    pub q: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmathRecord_ZetaEtaState {
    pub n: f64,
    pub eta: f64,
    pub sgn: f64,
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
                let __e2 = matches!(emath_rt::cmp_i64_f64((0i64), (scale)), Some(core::cmp::Ordering::Less | core::cmp::Ordering::Equal));
                __e2
            };
            if __ok0 {
                {
                    return Err(ConfigError::FailedPrecondition);
                }
            }
            let __ok1 = !{
                let __e1 = ((scale).is_finite());
                __e1
            };
            if __ok1 {
                {
                    return Err(ConfigError::FailedPrecondition);
                }
            }
            let __ok2 = !{
                let __e1 = ((bias).is_finite());
                __e1
            };
            if __ok2 {
                {
                    return Err(ConfigError::FailedPrecondition);
                }
            }
            let __post_ok0 = !{
                let __e2 = emath_rt::cmp_i64_f64((0i64), (scale)) == Some(core::cmp::Ordering::Less);
                __e2
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
                let __e2 = (self.scale) * (x);
                let __e4 = __e2 + (self.bias);
                __e4
            }
        }
    }
}

/// Example test: `score_is_seven`.
#[allow(clippy::float_cmp)]
#[test]
fn affine_scorer_score_is_seven() {
    {
        let bias = 4.0f64;
        let scale = 1.0f64;
        let x = 3.0f64;
        let affine_scorer = AffineScorer::new(scale, bias)
            .expect("constructor invariants must hold for this example");
        let actual = (affine_scorer.score(x));
        let score = actual;
        assert!({
            let __e2 = (score) == (7.0f64);
            __e2
        });
    }
}

/// Example test: `fractional_score`.
#[allow(clippy::float_cmp)]
#[test]
fn affine_scorer_fractional_score() {
    {
        let bias = 0.5f64;
        let scale = 2.0f64;
        let x = 1.5f64;
        let affine_scorer = AffineScorer::new(scale, bias)
            .expect("constructor invariants must hold for this example");
        let actual = (affine_scorer.score(x));
        let score = actual;
        assert!({
            let __e2 = (score) == (3.5f64);
            __e2
        });
    }
}
