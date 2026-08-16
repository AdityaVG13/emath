//!: assumption ledger.
//!
//! Classifies every assumption as Math (M), Numeric (N), System (S),
//! Environment (E) or Host (H) and exposes the ledger deterministically
//! for generated APIs and manifests.

use std::collections::BTreeMap;

use crate::EvidenceError;

/// Premise class.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PremiseClass {
    /// Mathematical assumption (identity, algebra, convergence).
    Math,
    /// Numeric assumption (precision, rounding, overflow behavior).
    Numeric,
    /// System assumption (threading, allocation, determinism).
    System,
    /// Environment assumption (toolchain, target, OS).
    Environment,
    /// Host assumption (caller contract, lifetime of inputs).
    Host,
}

impl PremiseClass {
    /// Stable class token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Math => "M",
            Self::Numeric => "N",
            Self::System => "S",
            Self::Environment => "E",
            Self::Host => "H",
        }
    }
}

/// Stable class token for receipts (`M`, `N`, `S`, `E`, `H`).
#[must_use]
pub fn premise_class_token(class: PremiseClass) -> &'static str {
    class.as_str()
}

/// One ledgered assumption.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Assumption {
    /// Assumption id (unique in the ledger).
    pub id: String,
    /// Statement.
    pub statement: String,
    /// Premise class.
    pub class: PremiseClass,
    /// Provenance (module/section that introduced it).
    pub provenance: String,
}

/// Assumption ledger.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AssumptionLedger {
    entries: BTreeMap<String, Assumption>,
}

impl AssumptionLedger {
    /// Registers an assumption; registering the same id under a
    /// different class is refused (`E-EVID-405`); re-registering the
    /// identical assumption is a no-op.
    pub fn register(&mut self, assumption: Assumption) -> Result<(), EvidenceError> {
        if let Some(existing) = self.entries.get(&assumption.id) {
            if existing.class != assumption.class {
                return Err(EvidenceError::new(
                    "E-EVID-405",
                    format!(
                        "assumption {} already registered under class {}",
                        assumption.id,
                        existing.class.as_str()
                    ),
                ));
            }
            return Ok(());
        }
        self.entries.insert(assumption.id.clone(), assumption);
        Ok(())
    }

    /// Assumptions in deterministic (id) order.
    #[must_use]
    pub fn assumptions(&self) -> Vec<&Assumption> {
        self.entries.values().collect()
    }

    /// Assumptions of one class, in id order.
    #[must_use]
    pub fn of_class(&self, class: PremiseClass) -> Vec<&Assumption> {
        self.entries
            .values()
            .filter(|assumption| assumption.class == class)
            .collect()
    }

    /// Count per class, in class order.
    #[must_use]
    pub fn counts(&self) -> [(PremiseClass, usize); 5] {
        [
            (PremiseClass::Math, self.of_class(PremiseClass::Math).len()),
            (
                PremiseClass::Numeric,
                self.of_class(PremiseClass::Numeric).len(),
            ),
            (
                PremiseClass::System,
                self.of_class(PremiseClass::System).len(),
            ),
            (
                PremiseClass::Environment,
                self.of_class(PremiseClass::Environment).len(),
            ),
            (PremiseClass::Host, self.of_class(PremiseClass::Host).len()),
        ]
    }

    /// Deterministic ledger token for manifests.
    #[must_use]
    pub fn canonical(&self) -> String {
        let rows: Vec<String> = self
            .assumptions()
            .iter()
            .map(|assumption| {
                format!(
                    "{}:{}:{}:{}",
                    assumption.id,
                    assumption.class.as_str(),
                    assumption.statement,
                    assumption.provenance
                )
            })
            .collect();
        rows.join(";")
    }
}
