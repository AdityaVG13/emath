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

/// Counter-stream unit-interval uniforms in [0, 1). `seed` is already the
/// local stream seed as an f64 bit pattern. Empty on `draws == 0`.
pub fn prob_unit_interval(seed: f64, draws: usize) -> Vec<f64> {
    if draws == 0 {
        return Vec::new();
    }
    let mut state = seed.to_bits();
    (0..draws).map(|_| prob_uniform01(&mut state)).collect()
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

