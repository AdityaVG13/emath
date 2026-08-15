#![forbid(unsafe_code)]

//! V7 g8 — Semantic calibration (spec 12).
//!
//! Behavioral examples constrain candidate worlds. This crate delivers:
//!
//! - deterministic example partitions (construction / validation /
//!   held-out / adversarial), keyed by content identity;
//! - finite-carrier operator-table fitting with a per-world record of
//!   satisfied and failed examples;
//! - a held-out challenge that no candidate is credited for if it saw the
//!   challenged examples during construction;
//! - semantic drift between fitted tables;
//! - world versioning with deterministic stamps, so a world invalidated by
//!   future examples is a new version, never a silent redefinition.

pub mod challenge;
pub mod drift;
pub mod fitting;
pub mod partition;
pub mod versioning;

pub use challenge::{hold_out_challenge, HeldOutChallenge, HeldOutResult};
pub use drift::{drift, SemanticDrift};
pub use fitting::{evaluate, fit_table, ExampleRecord, FitFailure, FittedTable};
pub use partition::{CalibrationExample, ExampleKind, PartitionedExamples};
pub use versioning::{WorldVersion, VERSION_SEED};

use emath_term::SymbolId;
use emath_world_ir::fnv1a64;

/// Deterministic content identity of an example.
#[must_use]
pub fn example_id(operator: &SymbolId, inputs: &[String], output: &str) -> u64 {
    fnv1a64(format!("example:{}:{}:{}", operator.0, inputs.join(","), output).as_bytes())
}

/// A calibrated world portfolio record (spec 12, "Result"): the fitted
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
    let version = WorldVersion::stamped("v1", &table.canonical());
    Ok(CalibrationRecord {
        table,
        challenge,
        records,
        version,
    })
}
