//!: revalidation.
//!
//! Evidence goes stale when the source, compiler, provider, checker,
//! target or assumptions change. A sweep marks affected records for
//! revalidation; records whose certificates remain independently valid
//! (checker-enforced, content-addressed) survive the sweep with no
//! re-run. Promotion of a stale record is refused (`E-EVID-505`).
//!
//! Stable codes:
//! - `E-EVID-504` reused by the store for double supersession;
//! - `E-EVID-505` stale record refused for promotion.

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
    /// Record ids whose certificates remain independently valid: the
    /// certificate is checker-enforced and content-addressed, so swept
    /// triggers do not stale it.
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

/// Sweeps the store: every stored record is either still valid or
/// stale. With no triggers nothing is invalidated; revoked records are
/// always stale (they cannot be promoted).
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

/// Promotion gate: a stale record cannot be promoted until it is
/// revalidated (`E-EVID-505`; unknown ids are `E-EVID-501`).
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests_support::sample_record;

    #[test]
    fn no_triggers_keep_resolved_evidence_valid() {
        let mut store = EvidenceStore::default();
        let id = store.register(sample_record()).unwrap().0;
        let report = revalidation_sweep(&store, &[], &RevalidationConfig::default());
        assert!(report.is_pass());
        assert_eq!(report.still_valid, vec![id.clone()]);
        assert!(require_promotable(&store, &report, &id).is_ok());
    }

    #[test]
    fn triggers_stale_evidence_but_not_independent_certificates() {
        let mut store = EvidenceStore::default();
        let id = store.register(sample_record()).unwrap().0;
        let mut renewed = sample_record();
        renewed.claim.id = "c1-v2".into();
        let keep = store
            .register_with_sources(renewed, std::slice::from_ref(&id))
            .unwrap()
            .0;

        let sweep_all = RevalidationConfig::default();
        let report = revalidation_sweep(
            &store,
            &[RevalidationTrigger::Compiler, RevalidationTrigger::Source],
            &sweep_all,
        );
        assert!(!report.is_pass());
        assert!(report.stale.contains(&id));
        assert!(report.stale.contains(&keep));
        assert_eq!(
            require_promotable(&store, &report, &id).unwrap_err().code,
            "E-EVID-505"
        );

        // Certificates declared independently valid survive the sweep.
        let config = RevalidationConfig {
            independently_valid: vec![keep.clone()],
        };
        let report = revalidation_sweep(&store, &[RevalidationTrigger::Source], &config);
        assert!(report.stale.contains(&id));
        assert!(report.still_valid.contains(&keep));
        assert!(require_promotable(&store, &report, &keep).is_ok());

        // Revoked records are always stale, even when declared valid.
        store.revoke(&keep).unwrap();
        let report = revalidation_sweep(&store, &[RevalidationTrigger::Source], &config);
        assert!(report.stale.contains(&keep));
    }

    #[test]
    fn unknown_id_is_refused_by_the_promotion_gate() {
        let store = EvidenceStore::default();
        let report = revalidation_sweep(&store, &[], &RevalidationConfig::default());
        assert_eq!(
            require_promotable(&store, &report, "fnv1a64:0000000000000000")
                .unwrap_err()
                .code,
            "E-EVID-501"
        );
    }
}
