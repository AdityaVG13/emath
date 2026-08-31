//! Finite-budget linear recurrences and formal power-series products.

/// Closed refusal surface for sequence evaluation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SequenceError {
    InvalidRecurrence,
    InvalidBudget,
    NonFinite,
}

impl SequenceError {
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::InvalidRecurrence => "E-SEQ-RECURRENCE",
            Self::InvalidBudget => "E-SEQ-BUDGET",
            Self::NonFinite => "E-SEQ-NONFINITE",
        }
    }
}

impl std::fmt::Display for SequenceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.code())
    }
}

impl std::error::Error for SequenceError {}

fn finite_count(value: f64) -> Result<usize, SequenceError> {
    if !value.is_finite() || value < 0.0 || value.fract() != 0.0 || value > 1_000_000.0 {
        return Err(SequenceError::InvalidBudget);
    }
    Ok(value as usize)
}

/// Materialize coefficients `0..=budget` of a homogeneous linear recurrence.
///
/// `recurrence[j]` multiplies the prior term at offset `j + 1`.
pub fn generate(
    initial: &[f64],
    recurrence: &[f64],
    budget: f64,
) -> Result<Vec<f64>, SequenceError> {
    let budget = finite_count(budget)?;
    if initial.is_empty()
        || recurrence.is_empty()
        || recurrence.len() > initial.len()
        || budget + 1 < initial.len()
    {
        return Err(SequenceError::InvalidRecurrence);
    }
    if initial
        .iter()
        .chain(recurrence)
        .any(|value| !value.is_finite())
    {
        return Err(SequenceError::NonFinite);
    }
    let values = crate::body::sequence_generate(initial, recurrence, budget as f64);
    if values.iter().any(|value| !value.is_finite()) {
        return Err(SequenceError::NonFinite);
    }
    Ok(values)
}

/// First `count` coefficients of the Cauchy product of two finite series.
pub fn convolve(left: &[f64], right: &[f64], count: f64) -> Result<Vec<f64>, SequenceError> {
    let count = finite_count(count)?;
    if count > left.len().saturating_add(right.len()).saturating_sub(1) {
        return Err(SequenceError::InvalidBudget);
    }
    if left.iter().chain(right).any(|value| !value.is_finite()) {
        return Err(SequenceError::NonFinite);
    }
    let result = crate::body::sequence_convolve(left, right, count as f64);
    if result.iter().any(|value| !value.is_finite()) {
        return Err(SequenceError::NonFinite);
    }
    Ok(result)
}
