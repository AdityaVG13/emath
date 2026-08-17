//! Held-out challenge: a candidate is not credited for
//! held-out performance if it saw those examples during construction.

use crate::fitting::{ExampleRecord, FittedTable, evaluate};
use crate::partition::{ExampleKind, PartitionedExamples};

/// Outcome of a held-out challenge for one fitted table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldOutResult {
    /// Construction coverage record.
    pub construction: ExampleRecord,
    /// Held-out coverage record.
    pub held_out: ExampleRecord,
    /// Whether the candidate survived the challenge.
    pub passed: bool,
    /// Machine-readable outcome in gate order:
    /// `construction:no-examples`, `construction-coverage:a<b`,
    /// `held-out:no-examples`, `held-out-coverage:a<b`, or `passed`.
    pub reason: String,
}

/// Coverage thresholds for the held-out challenge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeldOutChallenge {
    /// Minimum construction coverage (permille) to count as calibrated.
    pub min_construction_permille: u64,
    /// Minimum held-out coverage (permille) to count as general.
    pub min_held_out_permille: u64,
}

impl HeldOutChallenge {
    /// Runs the challenge. Both partitions must be non-vacuous: a world
    /// with no construction coverage is not calibrated, and a world with
    /// no held-out examples cannot be credited as general.
    #[must_use]
    pub fn evaluate(&self, table: &FittedTable, partition: &PartitionedExamples) -> HeldOutResult {
        let construction = evaluate(table, ExampleKind::Construction, partition.construction());
        let held_out = evaluate(table, ExampleKind::HeldOut, partition.held_out());

        let construction_fraction = construction.satisfied_fraction_permille();
        let held_out_fraction = held_out.satisfied_fraction_permille();

        let (passed, reason) = if construction.total_examples == 0 {
            (false, "construction:no-examples".to_string())
        } else if construction_fraction < self.min_construction_permille {
            (
                false,
                format!(
                    "construction-coverage:{}<{}",
                    construction_fraction, self.min_construction_permille
                ),
            )
        } else if held_out.total_examples == 0 {
            (false, "held-out:no-examples".to_string())
        } else if held_out_fraction < self.min_held_out_permille {
            (
                false,
                format!(
                    "held-out-coverage:{}<{}",
                    held_out_fraction, self.min_held_out_permille
                ),
            )
        } else {
            (true, "passed".to_string())
        };

        HeldOutResult {
            construction,
            held_out,
            passed,
            reason,
        }
    }
}

/// Convenience: runs a challenge with the given coverage thresholds.
#[must_use]
pub fn hold_out_challenge(
    table: &FittedTable,
    partition: &PartitionedExamples,
    min_construction_permille: u64,
    min_held_out_permille: u64,
) -> HeldOutResult {
    HeldOutChallenge {
        min_construction_permille,
        min_held_out_permille,
    }
    .evaluate(table, partition)
}
