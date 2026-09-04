//! B41 — finite-carrier game theory
//! nucleus. std-only.
//!
//! Honesty contract (the infinite-oracle
//! fence):
//! - Nash equilibrium is a CHECKABLE CLAIM — an assertion about a
//!   given profile — never a search promise. `is_nash_equilibrium`
//!   verifies a claimed profile; it does not enumerate equilibria.
//!   Support enumeration for MIXED Nash is the declared follow-up (a
//!   computation, distinct from the claim checker).
//! - The carrier is FINITE: `BimatrixGame` over row/column payoff
//!   matrices. Infinite/continuous games and general Nash oracles
//!   refuse by construction (the carrier does not represent them);
//!   that refusal is the honesty fence, not a missing feature to
//!   silently approximate.
//! - Best responses are reported as a SET: ties are real information,
//!   and a silent argmax pick would be an undisclosed choice.
//! - Mixed strategies are validated finite distributions (mass 1
//!   within 1e-9, non-negative, non-empty) — never renormalized (the
//!   `core::probability` discipline).
//! - Contract-first: the sema admission table does not admit game
//!   names yet; `.emath` models calling them refuse with the standard
//!   unknown-function diagnostic until the follow-up. The C8 `exists
//!   s2 in S` quantifier spelling already exists in the binder
//!   grammar (`in` is the owned token), so no parser change is needed.

/// An m×n payoff matrix in row-major order.
#[derive(Clone, Debug, PartialEq)]
pub struct PayoffMatrix {
    pub rows: usize,
    pub columns: usize,
    pub entries: Vec<f64>,
}

impl PayoffMatrix {
    #[must_use]
    pub fn at(&self, row: usize, column: usize) -> f64 {
        self.entries[row * self.columns + column]
    }

    fn shape_ok(&self) -> bool {
        self.entries.len() == self.rows * self.columns
    }
}

/// A two-player finite game: (row, column) strategies with separate
/// payoff matrices.
#[derive(Clone, Debug, PartialEq)]
pub struct BimatrixGame {
    pub row_payoffs: PayoffMatrix,
    pub column_payoffs: PayoffMatrix,
}

impl BimatrixGame {
    /// Row-strategy count (from the row matrix).
    #[must_use]
    pub fn rows(&self) -> usize {
        self.row_payoffs.rows
    }

    /// Column-strategy count.
    #[must_use]
    pub fn columns(&self) -> usize {
        self.row_payoffs.columns
    }

    /// Shape validation: both matrices rectangular with matching
    /// dimensions (a ragged carrier is not a game).
    pub fn validate(&self) -> Result<(), String> {
        if !self.row_payoffs.shape_ok() {
            return Err(format!(
                "row payoff matrix is ragged: {} entries for {}×{}",
                self.row_payoffs.entries.len(),
                self.row_payoffs.rows,
                self.row_payoffs.columns
            ));
        }
        if !self.column_payoffs.shape_ok() {
            return Err(format!(
                "column payoff matrix is ragged: {} entries for {}×{}",
                self.column_payoffs.entries.len(),
                self.column_payoffs.rows,
                self.column_payoffs.columns
            ));
        }
        if self.column_payoffs.rows != self.row_payoffs.rows
            || self.column_payoffs.columns != self.row_payoffs.columns
        {
            return Err("row and column payoff matrices must share the game shape".into());
        }
        Ok(())
    }

    /// THE CLAIM CHECKER (pure profiles): `(row, column)` is a Nash
    /// equilibrium iff neither player gains by a UNILATERAL deviation.
    /// Verifies the claim; never searches. Out-of-range profiles are
    /// `Err` (the claim is about a real profile; "false" would
    /// misreport a typo as game theory).
    pub fn is_nash_equilibrium(&self, row: usize, column: usize) -> Result<bool, String> {
        self.validate()?;
        if row >= self.rows() || column >= self.columns() {
            return Err(format!(
                "profile ({row}, {column}) is outside the {r}×{c} strategy space",
                r = self.rows(),
                c = self.columns()
            ));
        }
        let row_utility = self.row_payoffs.at(row, column);
        let column_utility = self.column_payoffs.at(row, column);
        // Row's unilateral deviations (column fixed).
        for alternative in 0..self.rows() {
            if self.row_payoffs.at(alternative, column) > row_utility {
                return Ok(false);
            }
        }
        // Column's unilateral deviations (row fixed).
        for alternative in 0..self.columns() {
            if self.column_payoffs.at(row, alternative) > column_utility {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Mixed-profile claim checker: `(row_mix, column_mix)` is a mixed
    /// Nash equilibrium iff each pure strategy IN THE SUPPORT of a
    /// player's mix is a best response to the opponent's mix (the
    /// standard support condition — a support strategy with a better
    /// unweighted alternative breaks equilibrium). Degenerate (pure)
    /// profiles satisfy the same condition.
    pub fn is_mixed_nash(
        &self,
        row_mix: &MixedStrategy,
        column_mix: &MixedStrategy,
    ) -> Result<bool, String> {
        self.validate()?;
        if row_mix.weights.len() != self.rows() {
            return Err(format!(
                "row mix has {} weights for {} row strategies",
                row_mix.weights.len(),
                self.rows()
            ));
        }
        if column_mix.weights.len() != self.columns() {
            return Err(format!(
                "column mix has {} weights for {} column strategies",
                column_mix.weights.len(),
                self.columns()
            ));
        }
        // Row's support strategies must maximize expected utility
        // against the column mix.
        let row_utilities: Vec<f64> = (0..self.rows())
            .map(|r| {
                expected_utility(
                    &self.row_payoffs,
                    &MixedStrategy::unit(r, self.rows()),
                    column_mix,
                )
                .unwrap_or(f64::NEG_INFINITY)
            })
            .collect();
        let row_best = row_utilities
            .iter()
            .fold(f64::NEG_INFINITY, |acc, value| acc.max(*value));
        for (strategy, &utility) in row_utilities.iter().enumerate() {
            if row_mix.weights[strategy] > 0.0 && utility < row_best - 1e-12 {
                return Ok(false);
            }
        }
        // Column's support strategies must maximize expected utility
        // against the row mix. CONVENTION (the pin that caught the
        // first draft's bug): column strategies index COLUMNS of the
        // payoff matrices, so column c's utility against row mix r is
        // Σᵢ rᵢ·column_payoffs[i][c] — expressed through the bilinear
        // helper as (column_payoffs, opponent-weights-on-ROWS, unit
        // column weights). Swapping the helper's arguments without
        // transposing the role convention silently scores the row
        // player's payoffs (probe: C-vs-D read 5 — the temptation —
        // instead of 0).
        let column_utilities: Vec<f64> = (0..self.columns())
            .map(|c| {
                expected_utility(
                    &self.column_payoffs,
                    row_mix,
                    &MixedStrategy::unit(c, self.columns()),
                )
                .unwrap_or(f64::NEG_INFINITY)
            })
            .collect();
        let column_best = column_utilities
            .iter()
            .fold(f64::NEG_INFINITY, |acc, value| acc.max(*value));
        for (strategy, &utility) in column_utilities.iter().enumerate() {
            if column_mix.weights[strategy] > 0.0 && utility < column_best - 1e-12 {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

/// A mixed strategy: validated weights over a finite carrier (mass 1
/// within 1e-9, non-negative — never renormalized).
#[derive(Clone, Debug, PartialEq)]
pub struct MixedStrategy {
    pub weights: Vec<f64>,
}

impl MixedStrategy {
    pub fn new(weights: Vec<f64>) -> Result<MixedStrategy, String> {
        if weights.is_empty() {
            return Err("mixed strategy over an empty carrier refuses".into());
        }
        let mut total = 0.0;
        for &weight in &weights {
            if !weight.is_finite() {
                return Err(format!("mixed strategy weight {weight} is not finite"));
            }
            if weight < 0.0 {
                return Err(format!("mixed strategy weight {weight} is negative"));
            }
            total += weight;
        }
        if (total - 1.0).abs() > 1e-9 {
            return Err(format!(
                "mixed strategy mass {total} is not 1 (mass is never silently renormalized)"
            ));
        }
        Ok(MixedStrategy { weights })
    }

    /// The degenerate mix putting all mass on one pure strategy.
    #[must_use]
    pub fn unit(index: usize, size: usize) -> MixedStrategy {
        let mut weights = vec![0.0; size];
        if index < size {
            weights[index] = 1.0;
        }
        MixedStrategy { weights }
    }
}

/// Best responses against a fixed opponent column: ALL maximizing row
/// indices, ascending (ties are a set — a silent argmax pick would be
/// an undisclosed choice). Empty/ragged carriers yield an empty set
/// rather than a fabricated index.
#[must_use]
pub fn best_responses(payoffs: &PayoffMatrix, column: usize) -> Vec<usize> {
    if !payoffs.shape_ok() || payoffs.rows == 0 || column >= payoffs.columns {
        return Vec::new();
    }
    let mut best = Vec::new();
    let mut best_value = f64::NEG_INFINITY;
    for row in 0..payoffs.rows {
        let value = payoffs.at(row, column);
        if value > best_value {
            best_value = value;
            best.clear();
            best.push(row);
        } else if (value - best_value).abs() <= 1e-12 {
            best.push(row);
        }
    }
    best
}

/// Expected utility of a mixed strategy pair over a payoff matrix:
/// `Σᵢⱼ rowᵢ·columnⱼ·payoff[i][j]` — the exact bilinear form.
pub fn expected_utility(
    payoffs: &PayoffMatrix,
    row_mix: &MixedStrategy,
    column_mix: &MixedStrategy,
) -> Result<f64, String> {
    if !payoffs.shape_ok() {
        return Err("payoff matrix is ragged".into());
    }
    if row_mix.weights.len() != payoffs.rows {
        return Err(format!(
            "row mix has {} weights for {} rows",
            row_mix.weights.len(),
            payoffs.rows
        ));
    }
    if column_mix.weights.len() != payoffs.columns {
        return Err(format!(
            "column mix has {} weights for {} columns",
            column_mix.weights.len(),
            payoffs.columns
        ));
    }
    let mut total = 0.0;
    for i in 0..payoffs.rows {
        for j in 0..payoffs.columns {
            total += row_mix.weights[i] * column_mix.weights[j] * payoffs.at(i, j);
        }
    }
    Ok(total)
}
