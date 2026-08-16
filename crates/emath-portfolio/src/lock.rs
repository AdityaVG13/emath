//! Portfolio generation replay: generation is replayable from
//! locks, seeds, budgets, provider versions, and canonical inputs.

use emath_world_ir::fnv1a64;

use crate::record::CandidateRecord;

/// Everything that pins a portfolio generation run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortfolioLock {
    /// Generation seed.
    pub seed: u64,
    /// Budget in host units.
    pub budget: u64,
    /// Provider versions, sorted by provider name.
    pub provider_versions: Vec<(String, String)>,
    /// Canonical inputs of the generation.
    pub canonical_inputs: String,
}

impl PortfolioLock {
    /// Builds a lock with deterministically sorted provider versions.
    #[must_use]
    pub fn new(
        seed: u64,
        budget: u64,
        mut provider_versions: Vec<(String, String)>,
        canonical_inputs: impl Into<String>,
    ) -> Self {
        provider_versions.sort_by(|left, right| left.0.cmp(&right.0));
        Self {
            seed,
            budget,
            provider_versions,
            canonical_inputs: canonical_inputs.into(),
        }
    }

    /// Deterministic canonical form.
    #[must_use]
    pub fn canonical(&self) -> String {
        let providers = self
            .provider_versions
            .iter()
            .map(|(provider, version)| format!("{provider}={version}"))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "lock:seed={}:budget={}:providers={}:inputs={}",
            self.seed, self.budget, providers, self.canonical_inputs
        )
    }

    /// Deterministic lock identity.
    #[must_use]
    pub fn id(&self) -> u64 {
        fnv1a64(self.canonical().as_bytes())
    }
}

/// Deterministic identity of a portfolio generation: lock identity plus
/// the record identities (sorted, so record order does not matter).
/// Equal inputs replay to the same identity; any lock or record change
/// produces a different one.
#[must_use]
pub fn replay_identity(lock: &PortfolioLock, records: &[CandidateRecord]) -> u64 {
    let mut identities = records
        .iter()
        .map(|record| record.identity)
        .collect::<Vec<_>>();
    identities.sort_unstable();
    let joined = identities
        .iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join(",");
    fnv1a64(format!("replay:{}:{}", lock.id(), joined).as_bytes())
}
