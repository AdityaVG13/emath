#![forbid(unsafe_code)]

//! Semantic calibration.
//!
//! Behavioral examples constrain candidate worlds: deterministic example
//! partitions, finite-carrier operator-table fitting with per-world
//! records, a held-out challenge, semantic drift, and versioned worlds
//! (invalidation → new version, never silent redefinition).

pub mod challenge;
pub mod drift;
pub mod fit_goal;
pub mod fitting;
pub mod partition;
pub mod versioning;

pub use challenge::{HeldOutChallenge, HeldOutResult, hold_out_challenge};
pub use drift::{SemanticDrift, drift};
pub use fit_goal::{
    AuthorityEscalation, ConfidenceInterval, FitGoal, FitMeasuredError, FitModel, FitOutcome,
    FitPayloadError, FitRow, Identifiability, IdentifiabilityProvider, NumericRankOracle,
    OptimizerMethod, ProvenanceHash, ResidualMethod, ResidualWeights, UnresolvedReason, escalate,
    fit, fnv1a64, jacobian_residuals, materialize_measured, provenance, weighted_residuals,
};
pub use fitting::{ExampleRecord, FitFailure, FittedTable, evaluate, fit_table};
pub use partition::{CalibrationExample, ExampleKind, PartitionedExamples};
pub use versioning::{VERSION_SEED, WorldVersion};

use emath_term::SymbolId;
use emath_world_ir::fnv1a64 as world_fnv1a64;

/// Deterministic content identity of an example.
#[must_use]
pub fn example_id(operator: &SymbolId, inputs: &[String], output: &str) -> u64 {
    world_fnv1a64(format!("example:{}:{}:{}", operator.0, inputs.join(","), output).as_bytes())
}

/// A calibrated world portfolio record ("Result"): the fitted
/// table, its per-partition example record, the held-out challenge
/// outcome, and a deterministic version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalibrationRecord {
    /// The fitted table.
    pub table: FittedTable,
    /// Held-out challenge outcome.
    pub challenge: HeldOutResult,
    /// Example records for every partition kind, in deterministic order.
    pub records: Vec<(ExampleKind, ExampleRecord)>,
    /// Deterministic version stamped from the table.
    pub version: WorldVersion,
}

/// Calibrates a candidate meaning: fit over construction examples, run
/// the held-out challenge, and record satisfaction/failure over every
/// partition.
pub fn calibrate(
    operator: &SymbolId,
    arity: usize,
    partition: &PartitionedExamples,
    min_construction_permille: u64,
    min_held_out_permille: u64,
) -> Result<CalibrationRecord, FitFailure> {
    let table = fit_table(operator, arity, partition.construction())?;
    let challenge = hold_out_challenge(
        &table,
        partition,
        min_construction_permille,
        min_held_out_permille,
    );
    let records = [
        ExampleKind::Construction,
        ExampleKind::Validation,
        ExampleKind::Adversarial,
        ExampleKind::HeldOut,
    ]
    .into_iter()
    .map(|kind| {
        let record = evaluate(&table, kind, partition.kind(kind));
        (kind, record)
    })
    .collect();
    let version = WorldVersion::stamped("stable", &table.canonical());
    Ok(CalibrationRecord {
        table,
        challenge,
        records,
        version,
    })
}
