//! Typed wrappers over the graph-traversal kernels (r2-graphs-masa
//! slice 1 + the slice-2 finite-weight gate).
//!
//! The single source of truth lives in [`crate::body`] (deterministic,
//! std-only, embedded verbatim into generated crates). This module adds
//! the typed refusal layer the reference interpreter surfaces:
//! `E-GRAPH-001` (non-square adjacency carrier), `E-GRAPH-002` (negative
//! edge weight under Dijkstra's precondition), `E-GRAPH-003` (source
//! vertex outside `0..n`), `E-GRAPH-004` (non-finite edge weight — the
//! all-finite numeric policy; `adj[u][v] != 0.0` is TRUE for NaN, so an
//! ungated NaN carrier would corrupt even BFS), `E-GRAPH-005` (a
//! negative cycle reachable from the source, Bellman-Ford only), and
//! `E-GRAPH-006` (a sparse triplet stream whose length is not a
//! multiple of three). Generated code observes
//! the same refusals as deterministic EMPTY kernel outputs — never a
//! wrong answer, never a panic. Determinism class: vertices are indices,
//! neighbor scans are ascending-index, Dijkstra ties break to the lowest
//! index; identical inputs are bit-identical.

/// Graph refusal. Closed set; codes are the language surface.
#[derive(Clone, Debug, PartialEq)]
pub enum GraphError {
    /// `E-GRAPH-001` — the adjacency carrier must be a square matrix.
    NonSquareAdjacency {
        /// Actual row count.
        rows: usize,
        /// Actual column count.
        cols: usize,
    },
    /// `E-GRAPH-002` — Dijkstra requires nonnegative edge weights.
    NegativeWeight,
    /// `E-GRAPH-003` — the traversal source is not a whole vertex
    /// index in `0..n`.
    SourceOutOfRange {
        /// The offending source value.
        source: f64,
        /// Vertex count of the carrier.
        vertices: usize,
    },
    /// `E-GRAPH-004` — an edge weight is non-finite (NaN or ±Inf): the
    /// carrier is not a well-formed weighted graph under the
    /// all-finite numeric policy.
    NonFiniteWeight,
    /// `E-GRAPH-005` — a negative cycle is reachable from the source:
    /// no shortest-path answer EXISTS (the relaxation diverges), so
    /// the solve refuses instead of fabricating distances.
    NegativeCycle,
    /// `E-GRAPH-006` — a sparse triplet stream is malformed (its
    /// length is not a multiple of three): the COO carrier is not
    /// well-formed.
    MalformedTriplets,
}

impl GraphError {
    /// Stable diagnostic code (the language surface).
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::NonSquareAdjacency { .. } => "E-GRAPH-001",
            Self::NegativeWeight => "E-GRAPH-002",
            Self::SourceOutOfRange { .. } => "E-GRAPH-003",
            Self::NonFiniteWeight => "E-GRAPH-004",
            Self::NegativeCycle => "E-GRAPH-005",
            Self::MalformedTriplets => "E-GRAPH-006",
        }
    }
}

impl std::fmt::Display for GraphError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonSquareAdjacency { rows, cols } => write!(
                formatter,
                "{code}: graph algorithms require a square adjacency matrix, \
                 got {rows}x{cols}",
                code = self.code()
            ),
            Self::NegativeWeight => write!(
                formatter,
                "{code}: shortest paths require nonnegative edge weights \
                 (Dijkstra's precondition)",
                code = self.code()
            ),
            Self::SourceOutOfRange { source, vertices } => write!(
                formatter,
                "{code}: traversal source {source} is not a vertex in 0..{vertices}",
                code = self.code()
            ),
            Self::NonFiniteWeight => write!(
                formatter,
                "{code}: adjacency weights must be finite (NaN/Inf is not a \
                 well-formed graph carrier)",
                code = self.code()
            ),
            Self::NegativeCycle => write!(
                formatter,
                "{code}: a negative cycle is reachable from the source — no \
                 shortest-path answer exists (the relaxation diverges)",
                code = self.code()
            ),
            Self::MalformedTriplets => write!(
                formatter,
                "{code}: the sparse triplet stream is malformed — its length \
                 must be a multiple of three (u, v, weight fields)",
                code = self.code()
            ),
        }
    }
}

impl std::error::Error for GraphError {}

/// Validate the carrier and source once, then delegate to the
/// [`crate::body`] kernel. The carrier is flat row-major with explicit
/// dimensions (the interpreter's representation); the kernel works on
/// the nested generated-crate representation.
fn validated_carrier(flat: &[f64], rows: usize, cols: usize) -> Result<Vec<Vec<f64>>, GraphError> {
    if rows != cols || rows == 0 || flat.len() != rows * cols {
        return Err(GraphError::NonSquareAdjacency { rows, cols });
    }
    if flat.iter().any(|weight| !weight.is_finite()) {
        // `!= 0.0` is TRUE for NaN: an ungated NaN carrier would make
        // BFS treat NaN as an edge and would propagate NaN distances.
        return Err(GraphError::NonFiniteWeight);
    }
    let mut nested = vec![vec![0.0; cols]; rows];
    for (index, value) in flat.iter().enumerate() {
        nested[index / cols][index % cols] = *value;
    }
    Ok(nested)
}

fn validated_source(source: usize, vertices: usize) -> Result<f64, GraphError> {
    if source >= vertices {
        return Err(GraphError::SourceOutOfRange {
            source: source as f64,
            vertices,
        });
    }
    Ok(source as f64)
}

/// BFS reachability mask from `source`: element `v` is 1.0 when `v` is
/// reachable (the source reaches itself), else 0.0.
///
/// # Errors
/// Non-square carrier (`E-GRAPH-001`) or a source outside `0..n`
/// (`E-GRAPH-003`) is typed.
pub fn reachability(
    flat: &[f64],
    rows: usize,
    cols: usize,
    source: usize,
) -> Result<Vec<f64>, GraphError> {
    let carrier = validated_carrier(flat, rows, cols)?;
    let source = validated_source(source, rows)?;
    Ok(crate::body::graph_reachable(&carrier, source))
}

/// BFS visit order from `source` (source first; neighbors discovered in
/// ascending index — breadth-first, never depth-first).
///
/// # Errors
/// Non-square carrier (`E-GRAPH-001`) or a source outside `0..n`
/// (`E-GRAPH-003`) is typed.
pub fn bfs_order(
    flat: &[f64],
    rows: usize,
    cols: usize,
    source: usize,
) -> Result<Vec<f64>, GraphError> {
    let carrier = validated_carrier(flat, rows, cols)?;
    let source = validated_source(source, rows)?;
    Ok(crate::body::graph_bfs_order(&carrier, source))
}

/// O(n²) selection Dijkstra: shortest distances from `source`;
/// unreachable vertices are +Inf.
///
/// # Errors
/// Non-square carrier (`E-GRAPH-001`), a source outside `0..n`
/// (`E-GRAPH-003`), or a negative edge weight (`E-GRAPH-002`,
/// Dijkstra's precondition) is typed.
pub fn dijkstra(
    flat: &[f64],
    rows: usize,
    cols: usize,
    source: usize,
) -> Result<Vec<f64>, GraphError> {
    let carrier = validated_carrier(flat, rows, cols)?;
    let source = validated_source(source, rows)?;
    if carrier
        .iter()
        .any(|row| row.iter().any(|weight| *weight < 0.0))
    {
        return Err(GraphError::NegativeWeight);
    }
    Ok(crate::body::graph_dijkstra(&carrier, source))
}

/// Out-degree per vertex: count of nonzero entries per row (0.0 is no
/// edge even in a weighted carrier; a self-loop counts). In-degree is
/// this op over the transposed carrier.
///
/// # Errors
/// Non-square carrier (`E-GRAPH-001`) is typed.
pub fn degree_out(flat: &[f64], rows: usize, cols: usize) -> Result<Vec<f64>, GraphError> {
    let carrier = validated_carrier(flat, rows, cols)?;
    Ok(crate::body::graph_degree_out(&carrier))
}

/// Unnormalized graph Laplacian `L = D − A` (flat row-major), D the
/// out-degree diagonal. The Laplacian of an undirected graph (symmetric
/// adjacency) is symmetric, so `eigvals(graph_laplacian(adj))` composes
/// through the existing symmetric eigen op.
///
/// # Errors
/// Non-square carrier (`E-GRAPH-001`), non-finite entries
/// (`E-GRAPH-004`), or a negative adjacency entry (`E-GRAPH-002` — a
/// negative weight is not a graph carrier for `D − A`) is typed.
pub fn laplacian(flat: &[f64], rows: usize, cols: usize) -> Result<Vec<f64>, GraphError> {
    let carrier = validated_carrier(flat, rows, cols)?;
    if carrier
        .iter()
        .any(|row| row.iter().any(|weight| *weight < 0.0))
    {
        return Err(GraphError::NegativeWeight);
    }
    let laplacian = crate::body::graph_laplacian(&carrier);
    Ok(laplacian.into_iter().flatten().collect())
}

/// Symmetrized adjacency `S = (A + Aᵀ)/2` (flat row-major) — the
/// weight-preserving convention (documented; NOT max, NOT boolean-or).
/// The output is a symmetric carrier, so `laplacian(symmetrize(A))`
/// composes through the existing symmetric-eigen path; symmetrization
/// is a USER choice, never a silent one inside `laplacian`/eigen.
///
/// # Errors
/// Non-square carrier (`E-GRAPH-001`), a negative adjacency entry
/// (`E-GRAPH-002` — same carrier law as `laplacian`), or a non-finite
/// entry (`E-GRAPH-004`) is typed.
pub fn symmetrize(flat: &[f64], rows: usize, cols: usize) -> Result<Vec<f64>, GraphError> {
    let carrier = validated_carrier(flat, rows, cols)?;
    if carrier
        .iter()
        .any(|row| row.iter().any(|weight| *weight < 0.0))
    {
        return Err(GraphError::NegativeWeight);
    }
    let symmetrized = crate::body::graph_symmetrize(&carrier);
    if symmetrized.is_empty() {
        // validated_carrier guarantees finiteness and squareness, so
        // an empty kernel result is unreachable; fail closed.
        return Err(GraphError::NonFiniteWeight);
    }
    Ok(symmetrized.into_iter().flatten().collect())
}

/// Bellman-Ford shortest distances from `source` (flat row-major):
/// negative edge weights are ADMITTED (the greedy invariant that
/// fails on them is Dijkstra's, not shortest-path theory's); a
/// negative cycle reachable from the source refuses `E-GRAPH-005`
/// (no answer exists); unreachable vertices are +Inf.
///
/// # Errors
/// Non-square carrier (`E-GRAPH-001`), a source outside `0..n`
/// (`E-GRAPH-003`), non-finite entries (`E-GRAPH-004`), or a reachable
/// negative cycle (`E-GRAPH-005`) is typed.
pub fn bellman_ford(
    flat: &[f64],
    rows: usize,
    cols: usize,
    source: usize,
) -> Result<Vec<f64>, GraphError> {
    let carrier = validated_carrier(flat, rows, cols)?;
    let source = validated_source(source, rows)?;
    let distances = crate::body::graph_bellman_ford(&carrier, source as usize);
    if distances.len() != rows {
        // The carrier and source are already validated, so an
        // unexpected length can only be the negative-cycle refusal.
        return Err(GraphError::NegativeCycle);
    }
    Ok(distances)
}

/// Sparse COO extraction: the dense adjacency as a flat triplet
/// stream `[u0, v0, w0, u1, v1, w1, ...]` in ascending (u, v) order;
/// explicit 0.0 entries are NOT edges and are skipped.
///
/// # Errors
/// Non-square carrier (`E-GRAPH-001`) or a non-finite entry
/// (`E-GRAPH-004`) is typed.
pub fn sparse_triplets(flat: &[f64], rows: usize, cols: usize) -> Result<Vec<f64>, GraphError> {
    let carrier = validated_carrier(flat, rows, cols)?;
    let triplets = crate::body::graph_sparse_triplets(&carrier);
    if triplets.is_empty() && carrier.iter().any(|row| row.iter().any(|w| *w != 0.0)) {
        // Unreachable (the carrier is validated); fail closed.
        return Err(GraphError::NonFiniteWeight);
    }
    Ok(triplets)
}

/// Sparse COO build: the dense adjacency from a flat triplet stream.
/// DUPLICATE (u, v) entries SUM (parallel edges add weights — the COO
/// build law, documented).
///
/// # Errors
/// A triplet index outside `0..n` (`E-GRAPH-003`), a non-finite weight
/// (`E-GRAPH-004`), or a stream whose length is not a multiple of
/// three (`E-GRAPH-006`) is typed. `n` must be a whole count ≥ 1
/// (a non-integer `n` refuses `E-GRAPH-003`).
pub fn sparse_from_triplets(n: f64, triplets: &[f64]) -> Result<Vec<f64>, GraphError> {
    if !n.is_finite() || n < 1.0 || n.fract() != 0.0 {
        return Err(GraphError::SourceOutOfRange {
            source: n,
            vertices: usize::MAX,
        });
    }
    let n = n as usize;
    // Classify the stream BEFORE the kernel call: the native kernel
    // signals every failure class the same way (an empty result), so
    // only a pre-pass can name the actual defect. Validation mirrors
    // the kernel's `graph_source_index` law per element — finite,
    // integral, 0 <= index < n — scanning u then v per triplet, then
    // the weight, in stream order. After this sweep the kernel call is
    // total: every one of its failure modes is pre-empted.
    if triplets.len() % 3 != 0 {
        return Err(GraphError::MalformedTriplets);
    }
    for fields in triplets.chunks_exact(3) {
        let [u, v, weight] = fields else {
            return Err(GraphError::MalformedTriplets);
        };
        for index in [u, v] {
            if !index.is_finite() || index.fract() != 0.0 || *index < 0.0 || *index >= n as f64 {
                return Err(GraphError::SourceOutOfRange {
                    source: *index,
                    vertices: n,
                });
            }
        }
        if !weight.is_finite() {
            return Err(GraphError::NonFiniteWeight);
        }
    }
    let built = crate::body::graph_sparse_from_triplets(n, triplets);
    Ok(built.into_iter().flatten().collect())
}

