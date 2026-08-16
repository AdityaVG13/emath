//!: evidence-level policy.
//!
//! Maps an E0–E5 requirement to the admissible producer/checker
//! combinations per claim class. The same claim class admits stricter
//! combinations at higher levels; `satisfied_by` decides whether a
//! concrete producer/checker pair meets the requirement.

use std::collections::BTreeMap;

use emath_ir::EvidenceLevel;

use crate::ir::{EvidenceKind, Independence};

/// One admissible producer/checker combination.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EvidenceEntry {
    /// Producer evidence kind.
    pub producer: EvidenceKind,
    /// Required checker independence (or `None` for no checker).
    pub checker: Independence,
}

impl EvidenceEntry {
    /// Stable token.
    #[must_use]
    pub fn token(self) -> String {
        format!("{}:{}", self.producer.as_str(), self.checker.as_str())
    }
}

/// Frozen evidence-level policy.
#[derive(Clone, Debug)]
pub struct EvidencePolicy {
    /// E0..E5 → admissible combinations per claim class.
    table: BTreeMap<String, Vec<EvidenceEntry>>,
    /// Claim classes known to the policy.
    classes: Vec<String>,
}

impl Default for EvidencePolicy {
    /// Default policy: five bars over the standard claim classes
    /// (`correctness`, `equivalence`, `performance`, `safety`).
    fn default() -> Self {
        let classes = vec![
            "correctness".to_string(),
            "equivalence".to_string(),
            "performance".to_string(),
            "safety".to_string(),
        ];
        let table = standard_table();
        Self { table, classes }
    }
}

impl EvidencePolicy {
    /// Admissible combinations for a `(level, claim class)` pair: the
    /// exact bar for that level. Lower-level evidence never satisfies a
    /// higher requirement.
    #[must_use]
    pub fn admissible(&self, level: EvidenceLevel, class: &str) -> Vec<EvidenceEntry> {
        self.table
            .get(&format!("{}:{class}", level.as_str()))
            .cloned()
            .unwrap_or_default()
    }

    /// Whether the concrete combination satisfies the requirement.
    #[must_use]
    pub fn satisfied_by(
        &self,
        level: EvidenceLevel,
        class: &str,
        producer: EvidenceKind,
        checker: Independence,
    ) -> bool {
        self.admissible(level, class)
            .iter()
            .any(|entry| entry.producer == producer && entry.checker == checker)
    }

    /// Claim classes the policy knows.
    #[must_use]
    pub fn classes(&self) -> &[String] {
        &self.classes
    }
}

/// The standard E0–E5 ladder over the standard claim classes. Each
/// higher bar admits a stronger producer/checker combination.
fn standard_table() -> BTreeMap<String, Vec<EvidenceEntry>> {
    let mut table = BTreeMap::new();
    for class in ["correctness", "equivalence", "performance", "safety"] {
        // E0: any claim, no checker.
        table.insert(format!("E0:{class}"), vec![]);
        // E1: self-reported measurement, no checker.
        table.insert(
            format!("E1:{class}"),
            vec![EvidenceEntry {
                producer: EvidenceKind::Measurement,
                checker: Independence::None,
            }],
        );
        // E2: structural/static producer with a cooperating checker.
        table.insert(
            format!("E2:{class}"),
            vec![EvidenceEntry {
                producer: EvidenceKind::Structural,
                checker: Independence::Cooperating,
            }],
        );
        // E3: differential or residual producer with an independent
        // checker plus falsifiers.
        table.insert(
            format!("E3:{class}"),
            vec![
                EvidenceEntry {
                    producer: EvidenceKind::Differential,
                    checker: Independence::Independent,
                },
                EvidenceEntry {
                    producer: EvidenceKind::Residual,
                    checker: Independence::Independent,
                },
            ],
        );
        // E4: interval/witness certificates, independently checked.
        table.insert(
            format!("E4:{class}"),
            vec![
                EvidenceEntry {
                    producer: EvidenceKind::Interval,
                    checker: Independence::Independent,
                },
                EvidenceEntry {
                    producer: EvidenceKind::Witness,
                    checker: Independence::Independent,
                },
            ],
        );
        // E5: machine-checked formal proof.
        table.insert(
            format!("E5:{class}"),
            vec![EvidenceEntry {
                producer: EvidenceKind::FormalProof,
                checker: Independence::Independent,
            }],
        );
    }
    table
}

/// Free-function accessors matching the module re-exports.
#[must_use]
pub fn requirement_for(
    policy: &EvidencePolicy,
    level: EvidenceLevel,
    class: &str,
) -> Vec<EvidenceEntry> {
    policy.admissible(level, class)
}

#[must_use]
pub fn admissible_combos(
    policy: &EvidencePolicy,
    level: EvidenceLevel,
    class: &str,
) -> Vec<EvidenceEntry> {
    policy.admissible(level, class)
}

#[must_use]
pub fn satisfied_by(
    policy: &EvidencePolicy,
    level: EvidenceLevel,
    class: &str,
    producer: EvidenceKind,
    checker: Independence,
) -> bool {
    policy.satisfied_by(level, class, producer, checker)
}
