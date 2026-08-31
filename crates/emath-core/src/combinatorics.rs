//! B17 (05 §3.3 #4) — combinatorics stdlib nucleus. std-only, no
//! external crates.
//!
//! Exact-integer semantics: counting values live on the `i128`
//! carrier (33! is the last factorial that fits); every overflow is a
//! typed refusal, never a wrapped value. `factorial` is the reference
//! contract for the admitted EMIR `Factorial` op; aligning the EMIR
//! compute path's overflow behavior with the reference's typed refusal
//! is a documented follow-up (exec-ir lane).
//!
//! `Permutation` is the finite carrier for orderings of `0..n`. C10:
//! the const-generic `Permutation<8>` is underivable at this language
//! stage, so the constructor is the runtime value form
//! `Permutation::new(n)` — the const-generic surface stays deferred
//! until value generics land. Combinations as a first-class type are
//! the combinatorial follow-up (binomial counting is admitted; the
//! combinatorial-number-system ranking is not in this slice).

/// Exact factorial on the `i128` carrier. `34!` is the first value
/// past the carrier and refuses by name.
pub fn factorial(n: u32) -> Result<i128, String> {
    let mut acc: i128 = 1;
    for k in 1..=n as i128 {
        acc = acc
            .checked_mul(k)
            .ok_or_else(|| format!("factorial({n}) overflows i128 — no exact carrier"))?;
    }
    Ok(acc)
}

/// Exact binomial coefficient `C(n, k)` on the `i128` carrier via the
/// multiplicative identity
/// `C(n, k) = ∏_{i=1..k} (n - k + i) / i`, evaluated with the
/// symmetry reduction `k → min(k, n − k)` (both spellings are the
/// same integer). The stepwise division is exact at every index (the
/// running product is `C(n − k + i, i)`), so no rounding is ever
/// introduced. `k > n` is the empty choice: `Ok(0)`. Any step past the
/// carrier refuses.
pub fn binomial(n: u64, k: u64) -> Result<i128, String> {
    if k > n {
        return Ok(0);
    }
    let k = k.min(n - k);
    let mut acc: i128 = 1;
    for i in 1..=k {
        acc = acc
            .checked_mul((n - k + i) as i128)
            .ok_or_else(|| format!("binomial({n}, {k}) overflows i128 — no exact carrier"))?;
        acc = acc
            .checked_div(i as i128)
            .ok_or_else(|| "binomial step division by zero — unreachable invariant".to_string())?;
    }
    Ok(acc)
}

/// A permutation of `0..n` as a finite carrier: `order[i]` is the
/// source index feeding output position `i`. Constructed validated
/// (`from_order`) or as the identity (`new`); the lexicographic
/// `successor` is the resumable continuation primitive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Permutation {
    order: Vec<u32>,
}

impl Permutation {
    /// The identity permutation of `0..n` — the C10 value-ctor
    /// workaround (`Permutation::new(8)`, not the underivable
    /// const-generic `Permutation<8>`).
    pub fn new(size: u32) -> Permutation {
        Permutation {
            order: (0..size).collect(),
        }
    }

    /// Validates that `order` is a bijection of `0..n` (every index in
    /// range exactly once); duplicates and out-of-range entries refuse.
    pub fn from_order(order: &[u32]) -> Result<Permutation, String> {
        let n = order.len() as u32;
        let mut seen = vec![false; n as usize];
        for &value in order {
            if value >= n {
                return Err(format!(
                    "permutation entry {value} out of range 0..{n} — not a bijection"
                ));
            }
            if seen[value as usize] {
                return Err(format!(
                    "permutation entry {value} repeats — not a bijection"
                ));
            }
            seen[value as usize] = true;
        }
        Ok(Permutation {
            order: order.to_vec(),
        })
    }

    /// Number of permuted indices.
    pub fn size(&self) -> u32 {
        self.order.len() as u32
    }

    /// The order rows (`order[i]` feeds position `i`).
    pub fn order(&self) -> &[u32] {
        &self.order
    }

    /// Source index feeding output position `index`. Callers guarantee
    /// `index < size` (the validated bijection is the invariant).
    pub fn apply(&self, index: u32) -> u32 {
        self.order[index as usize]
    }

    /// Lexicographic successor, or `None` when `self` is the last
    /// permutation of `0..n` — exhaustion is a named value, never a
    /// silent wrap to the identity (the continuation contract).
    pub fn successor(&self) -> Option<Permutation> {
        let n = self.order.len();
        if n < 2 {
            return None;
        }
        // Pivot: the rightmost ascent.
        let mut i = n - 2;
        while self.order[i] >= self.order[i + 1] {
            if i == 0 {
                return None;
            }
            i -= 1;
        }
        // Rightmost value exceeding the pivot.
        let mut j = n - 1;
        while self.order[j] <= self.order[i] {
            j -= 1;
        }
        let mut order = self.order.clone();
        order.swap(i, j);
        order[i + 1..].reverse();
        Some(Permutation { order })
    }
}

/// Enumeration under an explicit budget with a resumable continuation:
/// returns up to `budget` permutations of the lexicographic walk
/// STARTING AT `start` (the first batch includes `start` itself), and
/// the continuation — the next unvisited permutation, or `None` when
/// the walk is exhausted. Batches partition the walk exactly: resuming
/// from the continuation never repeats or skips an element. `budget 0`
/// yields an empty batch and `start` as the continuation.
pub fn enumerate_from(
    start: Permutation,
    budget: usize,
) -> (Vec<Permutation>, Option<Permutation>) {
    let mut batch = Vec::new();
    let mut continuation = None;
    let mut current = Some(start);
    while let Some(p) = current.take() {
        if batch.len() == budget {
            continuation = Some(p);
            break;
        }
        current = p.successor();
        batch.push(p);
    }
    (batch, continuation)
}
