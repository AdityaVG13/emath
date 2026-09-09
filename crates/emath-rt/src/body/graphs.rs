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
