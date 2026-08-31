//! Finite-carrier operator-table fitting and per-partition example
//! records.

use std::collections::BTreeMap;

use emath_term::SymbolId;
// The shared leaf type lives in world-ir (magnet relocation, o7a6);
// this crate owns the FITTING procedures and re-exports the type for
// stable paths.
pub use emath_world_ir::FittedTable;

use crate::partition::{CalibrationExample, ExampleKind};

/// Why a table fit failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FitFailure {
    /// An example's input arity does not match the declared arity.
    ArityMismatch {
        /// Example id that mismatched.
        example_id: u64,
        /// Declared arity.
        declared: usize,
        /// Example input count.
        actual: usize,
    },
    /// Two examples give conflicting outputs for the same inputs; a
    /// deterministic operator cannot be calibrated from both.
    Inconsistent {
        /// Id of the example that conflicted.
        example_id: u64,
        /// Inputs of the conflict.
        inputs: Vec<String>,
        /// Output established first.
        established_output: String,
        /// Output that conflicted with it.
        conflicting_output: String,
    },
}

/// Fits an arity-`arity` table for `operator` over construction examples.
///
/// Conflicting rows are a typed refusal: an operator meaning cannot
/// produce two outputs for one input.
pub fn fit_table(
    operator: &SymbolId,
    arity: usize,
    examples: &[CalibrationExample],
) -> Result<FittedTable, FitFailure> {
    let mut cells = BTreeMap::new();
    let mut first_conflict = None;
    for example in examples {
        if example.operator != *operator {
            continue;
        }
        if example.inputs.len() != arity {
            return Err(FitFailure::ArityMismatch {
                example_id: example.id,
                declared: arity,
                actual: example.inputs.len(),
            });
        }
        match cells.get(&example.inputs) {
            None => {
                cells.insert(example.inputs.clone(), example.output.clone());
            }
            Some(established) if *established == example.output => {}
            Some(established) if first_conflict.is_none() => {
                first_conflict = Some(FitFailure::Inconsistent {
                    example_id: example.id,
                    inputs: example.inputs.clone(),
                    established_output: established.clone(),
                    conflicting_output: example.output.clone(),
                });
            }
            Some(_) => {}
        }
    }
    match first_conflict {
        Some(failure) => Err(failure),
        None => Ok(FittedTable::from_cells(
            operator.clone(),
            arity,
            cells,
        )),
    }
}

/// Which examples of one partition a table satisfies or fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExampleRecord {
    /// The partition this record covers.
    pub kind: ExampleKind,
    /// Operator-matching examples in the partition.
    pub total_examples: usize,
    /// Example ids the table satisfies.
    pub satisfied_example_ids: Vec<u64>,
    /// Example ids the table fails.
    pub failed_example_ids: Vec<u64>,
}

impl ExampleRecord {
    /// Satisfied fraction in permille; vacuous records (no examples)
    /// report 1000 and are handled by the challenge explicitly.
    #[must_use]
    pub fn satisfied_fraction_permille(&self) -> u64 {
        if self.total_examples == 0 {
            return 1000;
        }
        (self.satisfied_example_ids.len() as u64 * 1000) / (self.total_examples as u64)
    }
}

/// Evaluates a table over one partition's operator-matching examples.
#[must_use]
pub fn evaluate(
    table: &FittedTable,
    kind: ExampleKind,
    examples: &[CalibrationExample],
) -> ExampleRecord {
    let mut satisfied = Vec::new();
    let mut failed = Vec::new();
    for example in examples {
        if example.operator != table.operator {
            continue;
        }
        match table.get(&example.inputs) {
            Some(output) if *output == example.output => satisfied.push(example.id),
            _ => failed.push(example.id),
        }
    }
    ExampleRecord {
        kind,
        total_examples: satisfied.len() + failed.len(),
        satisfied_example_ids: satisfied,
        failed_example_ids: failed,
    }
}
