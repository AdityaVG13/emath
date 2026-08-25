//! Revalidation.
//!
//! Evidence goes stale when source, compiler, provider, checker, target
//! or assumptions change. A sweep marks affected records for
//! revalidation; independently valid (checker-enforced, content-
//! addressed) certificates survive without re-run. Stale promotion
//! refuses (`E-EVID-505`).

use crate::{EvidenceError, EvidenceStore};

/// What invalidates and renews evidence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RevalidationTrigger {
    /// The source meaning changed.
    Source,
    /// The compiler changed.
    Compiler,
    /// A provider changed.
    Provider,
    /// The checker changed.
    Checker,
    /// The target profile changed.
    Target,
    /// An assumption changed.
    Assumption,
}

impl RevalidationTrigger {
    /// Stable trigger token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Compiler => "compiler",
            Self::Provider => "provider",
            Self::Checker => "checker",
            Self::Target => "target",
            Self::Assumption => "assumption",
        }
    }

    /// All triggers, in stable order.
    #[must_use]
    pub const fn all() -> [Self; 6] {
        [
            Self::Source,
            Self::Compiler,
            Self::Provider,
            Self::Checker,
            Self::Target,
            Self::Assumption,
        ]
    }
}

/// Revalidation configuration.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RevalidationConfig {
    /// Record ids whose certificates stay independently valid under
    /// swept triggers (checker-enforced, content-addressed).
    pub independently_valid: Vec<String>,
}

/// Sweep result.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RevalidationReport {
    /// Records invalidated by the sweep (require revalidation).
    pub stale: Vec<String>,
    /// Records that remain independently valid.
    pub still_valid: Vec<String>,
}

impl RevalidationReport {
    /// Whether every record survives without revalidation.
    #[must_use]
    pub fn is_pass(&self) -> bool {
        self.stale.is_empty()
    }
}

/// Sweep the store: every record is still valid or stale. No triggers
/// → nothing invalidated; revoked records are always stale.
#[must_use]
pub fn revalidation_sweep(
    store: &EvidenceStore,
    triggers: &[RevalidationTrigger],
    config: &RevalidationConfig,
) -> RevalidationReport {
    let mut report = RevalidationReport::default();
    let mut valid: Vec<String> = store
        .query_resolved()
        .into_iter()
        .map(|record| crate::store_address(record).0)
        .collect();
    valid.sort();
    valid.dedup();
    for id in valid {
        let independently_valid = config.independently_valid.iter().any(|kept| kept == &id);
        if store.is_revoked(&id) || (!triggers.is_empty() && !independently_valid) {
            report.stale.push(id);
        } else {
            report.still_valid.push(id);
        }
    }
    report
}

/// Promotion gate: stale records refuse until revalidated (`E-EVID-505`;
/// unknown ids `E-EVID-501`).
pub fn require_promotable(
    store: &EvidenceStore,
    report: &RevalidationReport,
    id: &str,
) -> Result<(), EvidenceError> {
    store.query(id)?;
    if report.stale.iter().any(|stale| stale == id) {
        return Err(EvidenceError::new(
            "E-EVID-505",
            format!("record {id} is stale and requires revalidation"),
        ));
    }
    Ok(())
}
