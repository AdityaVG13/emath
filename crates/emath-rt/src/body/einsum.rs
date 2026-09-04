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

