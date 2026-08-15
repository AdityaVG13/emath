//! Semantic drift (spec 12): when future examples invalidate a world, the
//! world is not silently redefined; a new version or semantic delta is
//! created and dependent artifacts are re-evaluated.

use emath_term::SymbolId;
use emath_world_ir::{fnv1a64, WorldId};

use crate::fitting::FittedTable;
use crate::versioning::WorldVersion;

/// A meaning change discovered from new examples.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticDrift {
    /// The drifting world.
    pub world: WorldId,
    /// The new version created for the changed meaning.
    pub next_version: WorldVersion,
    /// Canonical delta text: old table `→` new table.
    pub delta: String,
    /// Example ids whose meaning changed between the tables.
    pub changed_example_ids: Vec<u64>,
    /// Always true: dependents must re-evaluate (spec 12).
    pub dependents_re_evaluate: bool,
}

impl SemanticDrift {
    /// Deterministic canonical form.
    #[must_use]
    pub fn canonical(&self) -> String {
        format!(
            "drift:world={}:version={}:delta={}:changed={}",
            self.world.0,
            self.next_version.stamp,
            self.delta,
            self.changed_example_ids
                .iter()
                .map(u64::to_string)
                .collect::<Vec<_>>()
                .join(",")
        )
    }
}

/// Example content identity for a (operator, inputs, output) row.
#[must_use]
fn row_example_id(operator: &SymbolId, inputs: &[String], output: &str) -> u64 {
    fnv1a64(format!("example:{}:{}:{}", operator.0, inputs.join(","), output).as_bytes())
}

/// Records the drift between one world's old and new table for the same
/// operator. The old table stays intact; a new deterministic version is
/// stamped from the delta, and dependents are flagged for re-evaluation.
#[must_use]
pub fn drift(world: WorldId, old_table: &FittedTable, new_table: &FittedTable) -> SemanticDrift {
    assert_eq!(
        old_table.operator, new_table.operator,
        "drift compares tables of the same operator"
    );
    let mut changed = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for (inputs, output) in new_table.cells() {
        let changed_row = match old_table.get(inputs) {
            Some(old_output) => old_output != output,
            None => true,
        };
        if changed_row {
            let id = row_example_id(&new_table.operator, inputs, output);
            if seen.insert(id) {
                changed.push(id);
            }
        }
    }
    for (inputs, output) in old_table.cells() {
        if new_table.get(inputs).is_none() {
            let id = row_example_id(&old_table.operator, inputs, output);
            if seen.insert(id) {
                changed.push(id);
            }
        }
    }
    changed.sort_unstable();
    let delta = format!("{} -> {}", old_table.canonical(), new_table.canonical());
    let next_version = WorldVersion::stamped("next", &delta);
    SemanticDrift {
        world,
        next_version,
        delta,
        changed_example_ids: changed,
        dependents_re_evaluate: true,
    }
}
