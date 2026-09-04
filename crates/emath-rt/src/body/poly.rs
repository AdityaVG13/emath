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

