//! B22 (05 §3.3, scoped into `core::probability` per X10) —
//! information-theory stdlib nucleus. std-only, no external crates.
//!
//! Carrier and unit honesty:
//! - Probabilities are `f64` with DECLARED validation: non-negative,
//!   finite, total mass 1 within `1e-9`. Mass is never silently
//!   renormalized — a carrier that is not a distribution refuses.
//! - `entropy` / `kl_divergence` / `mutual_information` are pinned to
//!   BITS (Shannon's unit); `entropy_nats` is the separately named
//!   natural-log variant. The base is a declared distinction, never
//!   inferred.
//! - The discrete-vs-differential entropy distinction (criterion 4) is
//!   a FUNCTION distinction: the differential surface refuses by name
//!   because a density integral is not a mass sum — the giry
//!   probability world is the follow-up, and reusing the discrete sum
//!   for densities would be exactly the silent inference this module
//!   forbids.
//! - These are contract-first reference implementations: the sema call
//!   table does not admit the names yet (special-functions seam
//!   pattern); `.emath` models calling them refuse with the standard
//!   unknown-function diagnostic until the admission-table follow-up.
//!   B10's random-variable input row (`x: Random<Real> ~ Normal(0,
//!   1)`) is the same world-gated follow-up (needs a `FieldDecl`
//!   distribution annotation; `tree.rs` currently carries the
//!   reactions lane's in-flight work and is deliberately untouched).

/// Mass-validation shared by every function in this cell: a carrier
/// must be non-empty, finite, non-negative, and total to 1 within
/// `1e-9`. Never renormalizes.
fn validate_mass(distribution: &[f64], what: &str) -> Result<(), String> {
    if distribution.is_empty() {
        return Err(format!("{what} refuses: empty distribution"));
    }
    let mut total = 0.0;
    for &weight in distribution {
        if !weight.is_finite() {
            return Err(format!("{what} refuses: non-finite weight {weight}"));
        }
        if weight < 0.0 {
            return Err(format!("{what} refuses: negative weight {weight}"));
        }
        total += weight;
    }
    if (total - 1.0).abs() > 1e-9 {
        return Err(format!(
            "{what} refuses: total mass {total} is not 1 (mass is never silently renormalized)"
        ));
    }
    Ok(())
}

/// Shannon entropy `H(p) = −Σ p_i log2 p_i` in BITS, with the
/// `0·log2 0 := 0` convention (a zero-weight outcome carries no
/// information and must not poison the sum).
pub fn entropy(distribution: &[f64]) -> Result<f64, String> {
    entropy_in(distribution, 2.0, "entropy")
}

/// Shannon entropy in NATS (`−Σ p_i ln p_i`) — the declared natural-log
/// variant; `entropy` stays bits and the conversion is the caller's
/// documented `nats / ln 2`.
pub fn entropy_nats(distribution: &[f64]) -> Result<f64, String> {
    entropy_in(distribution, std::f64::consts::E, "entropy_nats")
}

/// Base-`base` entropy core; `base` is part of the contract spelling,
/// never a parameter a caller can leave implicit.
fn entropy_in(distribution: &[f64], base: f64, what: &str) -> Result<f64, String> {
    validate_mass(distribution, what)?;
    let log_base = base.ln();
    let mut total = 0.0;
    for &p in distribution {
        if p > 0.0 {
            total -= p * (p.ln() / log_base);
        }
    }
    Ok(total)
}

/// Kullback–Leibler divergence `D_KL(P ‖ Q) = Σ p_i log2(p_i / q_i)` in
/// BITS. Zero rows of `P` contribute 0 (the same convention as
/// entropy). A support violation — `p_i > 0` where `q_i = 0` — makes
/// the divergence `+∞`, which is NOT a finite value: it refuses by
/// name rather than returning a lie. Divergent carriers (`|P| ≠ |Q|`)
/// refuse: the pairing is row-wise by construction.
pub fn kl_divergence(p: &[f64], q: &[f64]) -> Result<f64, String> {
    if p.len() != q.len() {
        return Err(format!(
            "kl_divergence refuses: |P| = {} and |Q| = {} must pair row-wise",
            p.len(),
            q.len()
        ));
    }
    validate_mass(p, "kl_divergence(P, ·)")?;
    validate_mass(q, "kl_divergence(·, Q)")?;
    let mut total = 0.0;
    for (i, (&p_i, &q_i)) in p.iter().zip(q.iter()).enumerate() {
        if p_i == 0.0 {
            continue;
        }
        if q_i == 0.0 {
            return Err(format!(
                "kl_divergence refuses: support violation at row {i} — \
                 P[{i}] = {p_i} > 0 but Q[{i}] = 0 makes the divergence +∞"
            ));
        }
        total += p_i * ((p_i / q_i).ln() / 2.0_f64.ln());
    }
    Ok(total)
}

/// Mutual information `I(X;Y) = Σ p(x,y) log2(p(x,y) / (p_X(x) p_Y(y)))`
/// in BITS over a joint-mass table (rows = X values, columns = Y
/// values, rectangular, total mass 1). Zero cells contribute 0. The
/// non-negativity of MI is a THEOREM about the mathematics, not a
/// clamp: the implementation computes the honest sum and never clamps.
pub fn mutual_information(joint: &[Vec<f64>]) -> Result<f64, String> {
    if joint.is_empty() || joint[0].is_empty() {
        return Err("mutual_information refuses: empty joint table".into());
    }
    let columns = joint[0].len();
    let mut flat = Vec::with_capacity(joint.len() * columns);
    for row in joint {
        if row.len() != columns {
            return Err(
                "mutual_information refuses: ragged joint table (rows must be rectangular)".into(),
            );
        }
        flat.extend_from_slice(row);
    }
    validate_mass(&flat, "mutual_information")?;
    let mut marginal_x = vec![0.0; joint.len()];
    let mut marginal_y = vec![0.0; columns];
    for (x, row) in joint.iter().enumerate() {
        for (y, &p_xy) in row.iter().enumerate() {
            marginal_x[x] += p_xy;
            marginal_y[y] += p_xy;
        }
    }
    let mut total = 0.0;
    for (x, row) in joint.iter().enumerate() {
        for (y, &p_xy) in row.iter().enumerate() {
            if p_xy == 0.0 {
                continue;
            }
            let independent = marginal_x[x] * marginal_y[y];
            total += p_xy * ((p_xy / independent).ln() / 2.0_f64.ln());
        }
    }
    Ok(total)
}

/// Differential entropy is a DIFFERENT contract, not an inferred
/// variant: it integrates a DENSITY over a continuous carrier, which
/// the discrete mass-sum machinery must never silently impersonate.
/// Refuses until the giry probability world (measure-theoretic
/// carrier) lands.
pub fn entropy_differential(_density_carrier: &[f64]) -> Result<f64, String> {
    Err(
        "entropy_differential is not implemented: differential entropy is a measure-world \
         contract (density integral, not a mass sum) — the giry-probability world is the \
         named follow-up, and the discrete sum must never be silently reused for densities"
            .into(),
    )
}
