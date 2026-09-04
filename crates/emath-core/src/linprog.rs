//! LP/MILP solver nucleus + Pareto front (B24 + B36) — the finite-carrier machinery the goal
//! surface lowers into.
//!
//! Carrier and determinism contract:
//! - Variables are NONNEGATIVE (`x ≥ 0` is the declared standard-form
//!   carrier; free/bounded-below variables are a follow-up).
//! - LP: two-phase primal simplex with BLAND's rule (lowest-index
//!   entering column; ratio ties break to the lowest basis column) —
//!   pivot cycling is impossible and the pivot sequence is a pure
//!   function of the program, so runs are bit-deterministic.
//! - MILP: branch-and-bound, depth-first with the floor branch first,
//!   branching on the LOWEST-INDEX fractional integer variable; a
//!   node budget exists so exhaustion reports `NodeLimit` with the
//!   best known point instead of a false optimal claim.
//! - Statuses are named: `Infeasible` and `Unbounded` are answers,
//!   never garbage optima.
//!
//! The `.emath` surface (`objectives(pareto):` sections, goal-kind
//! admission) is the documented follow-up; this module is the solver
//! contract that surface lowers into.

/// Objective sense.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sense {
    Minimize,
    Maximize,
}

/// One linear constraint: `coeffs · x  (≤ | ≥ | =)  rhs`.
#[derive(Clone, Debug, PartialEq)]
pub struct Constraint {
    coeffs: Vec<f64>,
    kind: ConstraintKind,
    rhs: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ConstraintKind {
    Le,
    Ge,
    Eq,
}

impl Constraint {
    #[must_use]
    pub fn le(coeffs: &[f64], rhs: f64) -> Self {
        Self {
            coeffs: coeffs.to_vec(),
            kind: ConstraintKind::Le,
            rhs,
        }
    }

    #[must_use]
    pub fn ge(coeffs: &[f64], rhs: f64) -> Self {
        Self {
            coeffs: coeffs.to_vec(),
            kind: ConstraintKind::Ge,
            rhs,
        }
    }

    #[must_use]
    pub fn eq(coeffs: &[f64], rhs: f64) -> Self {
        Self {
            coeffs: coeffs.to_vec(),
            kind: ConstraintKind::Eq,
            rhs,
        }
    }
}

/// A linear program over the `x ≥ 0` carrier: `sense c·x` subject to
/// the declared constraints. Integrality (MILP) is opt-in per variable
/// via [`LinProg::with_integrality`].
#[derive(Clone, Debug, PartialEq)]
pub struct LinProg {
    sense: Sense,
    objective: Vec<f64>,
    constraints: Vec<Constraint>,
    integrality: Option<Vec<bool>>,
}

/// LP solution status.
#[derive(Clone, Debug, PartialEq)]
pub enum Solution {
    Optimal { primal: Vec<f64>, objective: f64 },
    Infeasible,
    Unbounded,
}

/// MILP solution status (branch-and-bound).
#[derive(Clone, Debug, PartialEq)]
pub enum MilpSolution {
    Optimal {
        primal: Vec<f64>,
        objective: f64,
    },
    Infeasible,
    Unbounded,
    /// The node budget was exhausted without proving optimality; the
    /// best known integer point is reported rather than a false
    /// optimal claim.
    NodeLimit {
        primal: Option<Vec<f64>>,
        objective: Option<f64>,
    },
}

enum Relaxed {
    Optimal(Vec<f64>, f64),
    Infeasible,
    Unbounded,
}

/// One simplex tableau row: coefficients over ALL columns plus rhs.
struct Row {
    coefficients: Vec<f64>,
    rhs: f64,
}

impl LinProg {
    #[must_use]
    pub fn minimize(objective: &[f64]) -> Self {
        Self {
            sense: Sense::Minimize,
            objective: objective.to_vec(),
            constraints: Vec::new(),
            integrality: None,
        }
    }

    #[must_use]
    pub fn maximize(objective: &[f64]) -> Self {
        Self {
            sense: Sense::Maximize,
            objective: objective.to_vec(),
            constraints: Vec::new(),
            integrality: None,
        }
    }

    #[must_use]
    pub fn constraint(mut self, constraint: Constraint) -> Self {
        self.constraints.push(constraint);
        self
    }

    /// Declare which variables must take integer values (MILP).
    #[must_use]
    pub fn with_integrality(mut self, integrality: Vec<bool>) -> Self {
        self.integrality = Some(integrality);
        self
    }

    /// Solve the LP relaxation (two-phase simplex, Bland's rule).
    #[must_use]
    pub fn solve(&self) -> Solution {
        match self.solve_relaxed() {
            Relaxed::Optimal(primal, objective) => Solution::Optimal { primal, objective },
            Relaxed::Infeasible => Solution::Infeasible,
            Relaxed::Unbounded => Solution::Unbounded,
        }
    }

    /// Solve as a MILP (branch-and-bound over the LP relaxation) when
    /// integrality is declared; `integer_tol` separates integral from
    /// fractional variables. Without integrality this IS the LP solve.
    #[must_use]
    pub fn solve_milp(&self, integer_tol: f64) -> MilpSolution {
        let Some(integrality) = &self.integrality else {
            return match self.solve_relaxed() {
                Relaxed::Optimal(primal, objective) => MilpSolution::Optimal { primal, objective },
                Relaxed::Infeasible => MilpSolution::Infeasible,
                Relaxed::Unbounded => MilpSolution::Unbounded,
            };
        };
        let maximizing = self.sense == Sense::Maximize;
        // Depth-first B&B. Open nodes carry their extra branching
        // constraints; the floor branch is pushed LAST so it is
        // explored first (deterministic order).
        let mut stack: Vec<Vec<Constraint>> = vec![self.constraints.clone()];
        let mut incumbent: Option<(f64, Vec<f64>)> = None;
        let mut nodes = 0_usize;
        const NODE_CAP: usize = 1_000_000;
        while let Some(extra) = stack.pop() {
            nodes += 1;
            if nodes > NODE_CAP {
                return match &incumbent {
                    Some((objective, primal)) => MilpSolution::NodeLimit {
                        primal: Some(primal.clone()),
                        objective: Some(*objective),
                    },
                    None => MilpSolution::NodeLimit {
                        primal: None,
                        objective: None,
                    },
                };
            }
            let mut node = self.clone();
            node.constraints = extra;
            let (primal, objective) = match node.solve_relaxed() {
                Relaxed::Optimal(primal, objective) => (primal, objective),
                Relaxed::Infeasible => continue,
                Relaxed::Unbounded => return MilpSolution::Unbounded,
            };
            // Bound prune: a node whose LP bound cannot STRICTLY beat
            // the incumbent is dropped.
            if let Some((best, _)) = &incumbent {
                let cannot_improve = if maximizing {
                    objective <= *best + 1e-12
                } else {
                    objective >= *best - 1e-12
                };
                if cannot_improve {
                    continue;
                }
            }
            // Branch on the LOWEST-INDEX fractional integer variable.
            let branch = primal
                .iter()
                .zip(integrality)
                .position(|(value, integral)| {
                    *integral && (value - value.round()).abs() > integer_tol
                });
            let Some(j) = branch else {
                // Integral within tolerance: snap the integer
                // variables and record the incumbent. The bound prune
                // above already guarantees this node's objective beats
                // the incumbent (within epsilon), so no separate
                // improvement gate is needed.
                let mut snapped = primal.clone();
                for (value, integral) in snapped.iter_mut().zip(integrality) {
                    if *integral {
                        *value = value.round();
                    }
                }
                incumbent = Some((objective, snapped));
                continue;
            };
            let unit = |index: usize| {
                let mut coefficients = vec![0.0; primal.len()];
                coefficients[index] = 1.0;
                coefficients
            };
            let mut down = node.constraints.clone();
            down.push(Constraint {
                coeffs: unit(j),
                kind: ConstraintKind::Le,
                rhs: primal[j].floor(),
            });
            let mut up = node.constraints.clone();
            up.push(Constraint {
                coeffs: unit(j),
                kind: ConstraintKind::Ge,
                rhs: primal[j].ceil(),
            });
            stack.push(up);
            stack.push(down);
        }
        match incumbent {
            Some((objective, primal)) => MilpSolution::Optimal { primal, objective },
            None => MilpSolution::Infeasible,
        }
    }

    /// Two-phase primal simplex (Bland's rule) over the `x ≥ 0`
    /// standard-form carrier. Returns the primal point in ORIGINAL
    /// variable order and the objective in the DECLARED sense.
    fn solve_relaxed(&self) -> Relaxed {
        let n = self.objective.len();
        let minimizing = self.sense == Sense::Minimize;
        let cost: Vec<f64> = if minimizing {
            self.objective.clone()
        } else {
            self.objective.iter().map(|value| -value).collect()
        };

        // Column layout: n structural, then per constraint
        // (Le: slack +1; Ge: surplus −1 then artificial; Eq:
        // artificial). Artificial columns exist for Ge/Eq rows.
        #[derive(Clone, Copy, PartialEq)]
        enum Column {
            Slack { row: usize, sign: f64 },
            Artificial { row: usize },
        }
        let mut columns: Vec<Column> = Vec::new();
        for (row, constraint) in self.constraints.iter().enumerate() {
            match constraint.kind {
                ConstraintKind::Le => columns.push(Column::Slack { row, sign: 1.0 }),
                ConstraintKind::Ge => {
                    columns.push(Column::Slack { row, sign: -1.0 });
                    columns.push(Column::Artificial { row });
                }
                ConstraintKind::Eq => columns.push(Column::Artificial { row }),
            }
        }
        let total = n + columns.len();
        let absolute = |index: usize| n + index;

        let mut rows: Vec<Row> = self
            .constraints
            .iter()
            .map(|constraint| Row {
                coefficients: {
                    let mut coefficients = vec![0.0; total];
                    coefficients[..n].copy_from_slice(&constraint.coeffs);
                    coefficients
                },
                rhs: constraint.rhs,
            })
            .collect();
        for (index, column) in columns.iter().enumerate() {
            match *column {
                Column::Slack { row, sign } => rows[row].coefficients[absolute(index)] = sign,
                Column::Artificial { row } => rows[row].coefficients[absolute(index)] = 1.0,
            }
        }

        // Initial basis: slack (+1) where present, artificial otherwise.
        let mut basis: Vec<usize> = vec![usize::MAX; rows.len()];
        for (index, column) in columns.iter().enumerate() {
            match *column {
                Column::Slack { row, sign: 1.0 } => basis[row] = absolute(index),
                _ => {}
            }
        }
        for (index, column) in columns.iter().enumerate() {
            if let Column::Artificial { row } = *column {
                basis[row] = absolute(index);
            }
        }

        // Phase 1 (only when artificials exist): minimize their sum.
        let artificial_columns: Vec<usize> = columns
            .iter()
            .enumerate()
            .filter_map(|(index, column)| {
                matches!(column, Column::Artificial { .. }).then_some(absolute(index))
            })
            .collect();
        if !artificial_columns.is_empty() {
            let phase1_cost: Vec<f64> = (0..total)
                .map(|column| u64::from(artificial_columns.contains(&column)) as f64)
                .collect();
            let phase1_outcome = simplex(&mut rows, &mut basis, &phase1_cost, &[], total);
            if phase1_outcome == RelaxedOutcome::Unbounded {
                // Phase-1 objective is bounded below by 0; unreachable.
                return Relaxed::Infeasible;
            }
            // Feasibility check: sum of artificial BASIC values.
            let artificial_sum: f64 = rows
                .iter()
                .zip(&basis)
                .filter(|(_, basis_column)| artificial_columns.contains(basis_column))
                .map(|(row, _)| row.rhs)
                .sum();
            if artificial_sum > 1e-9 {
                return Relaxed::Infeasible;
            }
            // Drive any artificial still basic (at value 0) out of the
            // basis; a row fully zero on non-artificial columns is
            // redundant and is zeroed (its basis becomes a free slack
            // at 0 — keep the artificial basic at 0, banned forever).
            for row_index in 0..rows.len() {
                if !artificial_columns.contains(&basis[row_index]) {
                    continue;
                }
                let replacement = (0..total).find(|&column| {
                    !artificial_columns.contains(&column)
                        && basis.iter().all(|&b| b != column)
                        && rows[row_index].coefficients[column].abs() > 1e-12
                });
                if let Some(column) = replacement {
                    pivot(&mut rows, &mut basis, row_index, column);
                } else {
                    // Redundant row: pin the artificial at 0 by leaving
                    // it basic but banned in phase 2.
                    continue;
                }
            }
        }

        // Phase 2: minimize the internal (always-minimizing) cost with
        // artificials banned from entering.
        let outcome = simplex(&mut rows, &mut basis, &cost, &artificial_columns, total);
        match outcome {
            RelaxedOutcome::Optimal => {}
            RelaxedOutcome::Unbounded => return Relaxed::Unbounded,
            RelaxedOutcome::Infeasible => return Relaxed::Infeasible,
        }

        // Extract the primal point and the objective in declared sense.
        let mut primal = vec![0.0; n];
        for (row, basis_column) in rows.iter().zip(&basis) {
            if *basis_column < n {
                primal[*basis_column] = row.rhs;
            }
        }
        let internal_value: f64 = cost
            .iter()
            .zip(&primal)
            .map(|(coefficient, value)| coefficient * value)
            .sum();
        let objective = if minimizing {
            internal_value
        } else {
            -internal_value
        };
        Relaxed::Optimal(primal, objective)
    }
}

enum RelaxedOutcome {
    Optimal,
    Unbounded,
    Infeasible,
}

impl PartialEq for RelaxedOutcome {
    fn eq(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (RelaxedOutcome::Optimal, RelaxedOutcome::Optimal)
                | (RelaxedOutcome::Unbounded, RelaxedOutcome::Unbounded)
                | (RelaxedOutcome::Infeasible, RelaxedOutcome::Infeasible)
        )
    }
}

/// One primal-simplex pivot: normalize the row, eliminate the column
/// elsewhere, record the new basis column.
fn pivot(rows: &mut [Row], basis: &mut [usize], row: usize, column: usize) {
    let pivot_value = rows[row].coefficients[column];
    for value in &mut rows[row].coefficients {
        *value /= pivot_value;
    }
    rows[row].rhs /= pivot_value;
    let pivot_row: Vec<f64> = rows[row].coefficients.clone();
    let pivot_rhs = rows[row].rhs;
    for (index, other) in rows.iter_mut().enumerate() {
        if index == row {
            continue;
        }
        let factor = other.coefficients[column];
        if factor != 0.0 {
            for (value, pivot_value) in other.coefficients.iter_mut().zip(pivot_row.iter()) {
                *value -= factor * pivot_value;
            }
            other.rhs -= factor * pivot_rhs;
        }
    }
    basis[row] = column;
}

/// Bland's-rule simplex over a prepared tableau. Entering: the
/// LOWEST-index column with negative reduced cost; leaving: the
/// minimum ratio, ties to the lowest basis column. Artificials stay
/// out via `banned`. Reduced costs are recomputed from the maintained
/// identity basis each pass (O(m) per column, fine at contract scale).
fn simplex(
    rows: &mut [Row],
    basis: &mut [usize],
    cost: &[f64],
    banned: &[usize],
    total: usize,
) -> RelaxedOutcome {
    loop {
        let mut entering: Option<usize> = None;
        for column in 0..total {
            if banned.contains(&column) || basis.contains(&column) {
                continue;
            }
            let column_cost = cost.get(column).copied().unwrap_or(0.0);
            let mut dot = 0.0;
            for (row, basis_column) in basis.iter().enumerate() {
                let basis_cost = cost.get(*basis_column).copied().unwrap_or(0.0);
                dot += basis_cost * rows[row].coefficients[column];
            }
            let reduced = column_cost - dot;
            if reduced < -1e-9 {
                entering = Some(column);
                break; // Bland: lowest index wins
            }
        }
        let Some(entering) = entering else {
            return RelaxedOutcome::Optimal;
        };
        // Ratio test; ties break to the lowest basis column (Bland).
        let mut leaving: Option<(usize, f64)> = None;
        for (row, tableau_row) in rows.iter().enumerate() {
            let entry = tableau_row.coefficients[entering];
            if entry <= 1e-12 {
                continue;
            }
            let ratio = tableau_row.rhs / entry;
            let take = match leaving {
                None => true,
                Some((best_row, best_ratio)) => {
                    if (ratio - best_ratio).abs() <= 1e-12 {
                        basis[row] < basis[best_row]
                    } else {
                        ratio < best_ratio
                    }
                }
            };
            if take {
                leaving = Some((row, ratio));
            }
        }
        let Some((row, _)) = leaving else {
            return RelaxedOutcome::Unbounded;
        };
        pivot(rows, basis, row, entering);
    }
}

/// Nondominated set (Pareto front) over 2-objective MINIMIZATION
/// points: `a` dominates `b` iff `a ≤ b` componentwise and `a ≠ b`.
/// Duplicates are mutually nondominated and collapse to one entry;
/// membership and order are stable in first-occurrence order.
#[must_use]
pub fn pareto_front(points: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut kept: Vec<(f64, f64)> = Vec::new();
    for &(x, y) in points {
        // Distinct + weakly-better already implies strict dominance:
        // `a ≤ b` componentwise with `a ≠ b` must be strict somewhere.
        let dominated_by_other = points
            .iter()
            .any(|&(ox, oy)| (ox, oy) != (x, y) && ox <= x && oy <= y);
        if !dominated_by_other && !kept.contains(&(x, y)) {
            kept.push((x, y));
        }
    }
    kept
}
