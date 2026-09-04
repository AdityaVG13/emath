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

