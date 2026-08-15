//!: plan identity and cache.
//!
//! The canonical plan identity binds goal, policy, provider set and target.
//! The cache holds plans by identity; a changed provider/descriptor
//! fingerprint (versions, evidence ceilings, determinism) changes the
//! identity, so stale entries are detected and never served.

use emath_core::{fnv1a64_bytes, ContentId};
use emath_ir::ResolutionPlan;
use std::collections::BTreeMap;

/// Canonical plan identity: binds goal schema, planner policy, provider
/// set (sorted ids) and target family.
#[must_use]
pub fn plan_identity(
    goal_canonical: &str,
    policy: &str,
    providers: &[String],
    target: &str,
) -> ContentId {
    let mut payload = String::new();
    payload.push_str("plan:v1:");
    payload.push_str(goal_canonical);
    payload.push('\n');
    payload.push_str(policy);
    payload.push('\n');
    for provider in providers {
        payload.push_str(provider);
        payload.push('\n');
    }
    payload.push_str(target);
    ContentId(format!(
        "fnv1a64:{:016x}",
        fnv1a64_bytes(payload.as_bytes())
    ))
}

/// One fingerprint row per provider (versioned, evidence, determinism).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderFingerprint {
    /// Provider id.
    pub id: String,
    /// Descriptor version.
    pub version: String,
    /// Evidence ceiling.
    pub evidence: u8,
    /// Determinism flag.
    pub deterministic: bool,
}

impl ProviderFingerprint {
    /// Canonical row.
    #[must_use]
    pub fn canonical(&self) -> String {
        format!(
            "{}@{}:ev{}:det{}",
            self.id, self.version, self.evidence, self.deterministic
        )
    }
}

/// Fingerprint of a provider set: stable only modulo descriptor changes.
#[must_use]
pub fn provider_set_fingerprint(rows: &[ProviderFingerprint]) -> u64 {
    let mut payload = String::new();
    let mut rows: Vec<&ProviderFingerprint> = rows.iter().collect();
    rows.sort_by(|left, right| left.id.cmp(&right.id));
    for row in &rows {
        payload.push_str(&row.canonical());
        payload.push('\n');
    }
    fnv1a64_bytes(payload.as_bytes())
}

/// One cached plan plus the provider fingerprint it was planned under.
#[derive(Clone, Debug)]
struct CacheEntry {
    fingerprint: u64,
    plan: ResolutionPlan,
}

/// Plan cache keyed by canonical identity; entries record the provider
/// fingerprint they were planned under, so descriptor changes invalidate
/// them.
#[derive(Clone, Debug, Default)]
pub struct PlanCache {
    entries: BTreeMap<ContentId, CacheEntry>,
}

impl PlanCache {
    /// Empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts a plan under its identity and current fingerprint.
    pub fn insert(&mut self, identity: &ContentId, plan: ResolutionPlan, fingerprint: u64) {
        self.entries
            .insert(identity.clone(), CacheEntry { fingerprint, plan });
    }

    /// Looks up a plan by identity.
    #[must_use]
    pub fn lookup(&self, identity: &ContentId) -> Option<&ResolutionPlan> {
        self.entries.get(identity).map(|entry| &entry.plan)
    }

    /// True when a cached entry exists and its fingerprint still matches
    /// the current provider set (drift detection).
    #[must_use]
    pub fn is_fresh(&self, identity: &ContentId, provider_fingerprint: u64) -> bool {
        self.entries
            .get(identity)
            .is_some_and(|entry| entry.fingerprint == provider_fingerprint)
    }

    /// Invalidates entries whose fingerprint no longer matches the current
    /// provider set; returns the number of dropped entries.
    pub fn invalidate_stale(&mut self, provider_fingerprint: u64) -> usize {
        let retained: BTreeMap<ContentId, CacheEntry> = self
            .entries
            .iter()
            .filter(|(_, entry)| entry.fingerprint == provider_fingerprint)
            .map(|(identity, entry)| (identity.clone(), entry.clone()))
            .collect();
        let dropped = self.entries.len() - retained.len();
        self.entries = retained;
        dropped
    }

    /// Number of cached plans.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the cache is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_binds_policy_providers_target_goal() {
        let providers = vec!["a".to_string(), "b".to_string()];
        let first = plan_identity("goal:g1", "policy:p1", &providers, "rust-library");
        assert_eq!(
            plan_identity("goal:g1", "policy:p1", &providers, "rust-library"),
            first
        );
        assert_ne!(
            plan_identity("goal:g1", "policy:p1", &["c".to_string()], "rust-library"),
            first
        );
        assert_ne!(
            plan_identity("goal:g2", "policy:p1", &providers, "rust-library"),
            first
        );
        assert_ne!(
            plan_identity("goal:g1", "policy:p1", &providers, "python"),
            first
        );
    }

    #[test]
    fn fingerprint_tracks_descriptor_changes() {
        let base = ProviderFingerprint {
            id: "p1".into(),
            version: "1.2.0".into(),
            evidence: 2,
            deterministic: true,
        };
        let base_rows = vec![base.clone()];
        let stable = provider_set_fingerprint(&base_rows);
        assert_eq!(provider_set_fingerprint(&base_rows), stable);
        let mut drifted = base.clone();
        drifted.deterministic = false;
        assert_ne!(provider_set_fingerprint(&[drifted]), stable);
        let mut drifted = base.clone();
        drifted.evidence = 3;
        assert_ne!(provider_set_fingerprint(&[drifted]), stable);
        assert_ne!(
            provider_set_fingerprint(&[base.clone(), base.clone()]),
            stable
        );
    }

    #[test]
    fn cache_invalidates_on_provider_change() {
        let mut cache = PlanCache::new();
        let providers = vec!["p1".to_string()];
        let rows = vec![ProviderFingerprint {
            id: "p1".into(),
            version: "1.2.0".into(),
            evidence: 2,
            deterministic: true,
        }];
        let fingerprint = provider_set_fingerprint(&rows);
        let identity = plan_identity("goal:g1", "policy:p1", &providers, "rust-library");
        let summary = emath_ir::ResolutionPlan {
            schema: emath_core::SchemaId("emath.resolution-plan.v1".into()),
            plan_id: identity.clone(),
            goal: emath_ir::GoalId(0),
            policy: "policy:p1".into(),
            artifact_class: "native".into(),
            nodes: BTreeMap::new(),
            root: emath_ir::PlanNodeId(0),
            excluded_candidates: vec![],
        };
        cache.insert(&identity, summary, fingerprint);
        assert!(cache.is_fresh(&identity, fingerprint));
        assert_eq!(
            cache.invalidate_stale(provider_set_fingerprint(&[ProviderFingerprint {
                id: "p1".into(),
                version: "1.3.0".into(),
                evidence: 2,
                deterministic: true,
            }])),
            1
        );
        assert!(cache.is_empty());
        assert!(!cache.is_fresh(&identity, fingerprint));
        cache.insert(
            &identity,
            emath_ir::ResolutionPlan {
                schema: emath_core::SchemaId("emath.resolution-plan.v1".into()),
                plan_id: identity.clone(),
                goal: emath_ir::GoalId(0),
                policy: "policy:p1".into(),
                artifact_class: "native".into(),
                nodes: BTreeMap::new(),
                root: emath_ir::PlanNodeId(0),
                excluded_candidates: vec![],
            },
            fingerprint,
        );
        assert!(cache.lookup(&identity).is_some());
    }
}
