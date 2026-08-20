//! Portfolio selection modes: user-selected, policy-selected,
//! host-experiment-selected, authority-threshold-selected,
//! lowest-cost-satisfying, or a Pareto portfolio with no single winner.
//!
//! Selection never raises meaning authority (no hidden escalation): a
//! fast candidate remains only as authoritative as its checks.

use emath_world_ir::WorldId;

use crate::record::CandidateRecord;
use crate::Authority;

/// Deterministic linear weights for policy-shaped selection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SelectionWeights {
    /// Weight for satisfied-law fraction (0..=1).
    pub law: f64,
    /// Weight for the evidence axis of the score vector.
    pub evidence: f64,
    /// Weight for the host-objective (utility) axis.
    pub host: f64,
    /// Weight for (penalty on) execution cost.
    pub cost: f64,
}

impl SelectionWeights {
    /// Balanced weights over law, evidence, host, and cost.
    #[must_use]
    pub fn balanced() -> Self {
        Self {
            law: 1.0,
            evidence: 1.0,
            host: 1.0,
            cost: 1.0,
        }
    }
}

/// How a portfolio selects among candidates.
#[derive(Debug, Clone, PartialEq)]
pub enum SelectionPolicy {
    /// Pick the named world, if it is viable.
    UserSelected {
        /// Desired world identity.
        world_id: WorldId,
    },
    /// The viable candidate with the lowest execution cost.
    LowestCostSatisfying,
    /// First viable candidate whose meaning authority meets the bar,
    /// ordered by balanced score.
    AuthorityThreshold {
        /// Minimum authority.
        minimum: Authority,
    },
    /// The viable candidate with the best host-objective (utility) score.
    HostExperiment,
    /// Deterministic weighted policy score.
    Policy(SelectionWeights),
    /// Pareto portfolio over (minimize cost/memory, maximize
    /// law/evidence/utility); no single winner is implied.
    Pareto,
}

/// Result of applying a selection policy.
#[derive(Debug, Clone, PartialEq)]
pub enum SelectionOutcome {
    /// One winner plus its rank in the viable ordering.
    Selected {
        /// Winner world identity.
        world_id: WorldId,
        /// Winner record identity.
        record_identity: u64,
        /// Rank among viable candidates (0 = top).
        rank: usize,
    },
    /// No viable candidate met the policy.
    NoWinner,
    /// Nondominated set (Pareto portfolio); winners are record
    /// identities, ascending; no single winner is implied.
    ParetoPortfolio {
        /// Nondominated record identities, ascending.
        winners: Vec<u64>,
    },
}

/// Applies a selection policy over records. Only viable records
/// participate; ties break toward the lower world identity everywhere.
#[must_use]
pub fn select(records: &[CandidateRecord], policy: &SelectionPolicy) -> SelectionOutcome {
    let viable = records
        .iter()
        .filter(|record| record.viable())
        .collect::<Vec<_>>();
    match policy {
        SelectionPolicy::Pareto => pareto_portfolio(&viable),
        SelectionPolicy::UserSelected { world_id } => {
            let Some(record) = viable.iter().find(|record| record.world_id == *world_id) else {
                return SelectionOutcome::NoWinner;
            };
            let rank = rank_in(viable.clone(), record, balanced_key);
            selected(record, rank)
        }
        SelectionPolicy::LowestCostSatisfying => {
            let ordered = order_by(viable.clone(), cost_key);
            let Some(record) = ordered.first() else {
                return SelectionOutcome::NoWinner;
            };
            selected(record, 0)
        }
        SelectionPolicy::AuthorityThreshold { minimum } => {
            let mut eligible = viable
                .iter()
                .copied()
                .filter(|record| record.authority >= *minimum)
                .collect::<Vec<_>>();
            if eligible.is_empty() {
                return SelectionOutcome::NoWinner;
            }
            eligible.sort_by(|left, right| by_balanced_then_world(left, right));
            selected(eligible[0], 0)
        }
        SelectionPolicy::HostExperiment => {
            let ordered = order_by(viable.clone(), |record| record.score.utility);
            let Some(record) = ordered.first() else {
                return SelectionOutcome::NoWinner;
            };
            selected(record, 0)
        }
        SelectionPolicy::Policy(weights) => {
            let ordered = order_by(viable.clone(), |record| policy_key(record, weights));
            let Some(record) = ordered.first() else {
                return SelectionOutcome::NoWinner;
            };
            selected(record, 0)
        }
    }
}

/// Builds a `Selected` outcome.
fn selected(record: &CandidateRecord, rank: usize) -> SelectionOutcome {
    SelectionOutcome::Selected {
        world_id: record.world_id,
        record_identity: record.identity,
        rank,
    }
}

/// Rank of `record` in the key-ordered viable list (0 = top).
fn rank_in(
    candidates: Vec<&CandidateRecord>,
    record: &CandidateRecord,
    key: impl Fn(&CandidateRecord) -> f64,
) -> usize {
    order_by(candidates, key)
        .iter()
        .position(|candidate| candidate.world_id == record.world_id)
        .unwrap_or(0)
}

/// Sorts candidates by `key` descending (`total_cmp`), ties by world id
/// ascending.
fn order_by(
    mut candidates: Vec<&CandidateRecord>,
    key: impl Fn(&CandidateRecord) -> f64,
) -> Vec<&CandidateRecord> {
    candidates.sort_by(|left, right| {
        key(right)
            .total_cmp(&key(left))
            .then_with(|| left.world_id.cmp(&right.world_id))
    });
    candidates
}

/// Balanced linear policy key.
fn balanced_key(record: &CandidateRecord) -> f64 {
    policy_key(record, &SelectionWeights::balanced())
}

/// Weighted deterministic policy key. Costs are host units cast to the
/// deterministic ordering domain; precision loss is bounded and fine.
#[allow(clippy::cast_precision_loss)]
fn policy_key(record: &CandidateRecord, weights: &SelectionWeights) -> f64 {
    weights.law * (record.law_permille() as f64 / 1000.0)
        + weights.evidence * record.score.evidence
        + weights.host * record.score.utility
        - weights.cost * record.score.cost
}

/// Lowest-cost ordering key (negative so descending sort picks minimum).
#[allow(clippy::cast_precision_loss)]
fn cost_key(record: &CandidateRecord) -> f64 {
    -(record.execution_cost as f64)
}

/// Ordering used for authority-threshold selection.
fn by_balanced_then_world(left: &CandidateRecord, right: &CandidateRecord) -> std::cmp::Ordering {
    balanced_key(right)
        .total_cmp(&balanced_key(left))
        .then_with(|| left.world_id.cmp(&right.world_id))
}

/// Pareto front over (minimize execution/memory cost, maximize
/// law/evidence/utility). Winner list is deterministic.
fn pareto_portfolio(viable: &[&CandidateRecord]) -> SelectionOutcome {
    let winners = viable
        .iter()
        .filter(|record| !viable.iter().any(|other| dominates(other, record)))
        .map(|record| record.identity)
        .collect::<Vec<_>>();
    if winners.is_empty() {
        SelectionOutcome::NoWinner
    } else {
        let mut winners = winners;
        winners.sort_unstable();
        SelectionOutcome::ParetoPortfolio { winners }
    }
}

/// Whether `left` is at least as good as `right` on every Pareto axis and
/// strictly better on at least one.
fn dominates(left: &CandidateRecord, right: &CandidateRecord) -> bool {
    let not_worse = left.execution_cost <= right.execution_cost
        && left.memory_cost <= right.memory_cost
        && left.law_permille() >= right.law_permille()
        && left.score.evidence.total_cmp(&right.score.evidence).is_ge()
        && left.score.utility.total_cmp(&right.score.utility).is_ge();
    let strict = left.execution_cost < right.execution_cost
        || left.memory_cost < right.memory_cost
        || left.law_permille() > right.law_permille()
        || matches!(
            left.score.evidence.total_cmp(&right.score.evidence),
            std::cmp::Ordering::Greater
        )
        || matches!(
            left.score.utility.total_cmp(&right.score.utility),
            std::cmp::Ordering::Greater
        );
    not_worse && strict
}
