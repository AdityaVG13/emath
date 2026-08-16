//!: provider registry.
//!
//! Loads static, sandboxed, external-process and remote descriptors under
//! an isolation policy and an optional implementation lock. Denied and
//! lock-mismatched registrations produce typed refusals
//! (`E-PROV-510`/`E-PROV-511`); lookups are deterministic.

use crate::descriptor::{CapabilityTable, ProviderIsolation, ProviderLock};
use emath_core::ContentId;
use std::collections::BTreeMap;

/// Registration policy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegistryConfig {
    /// Isolation kinds allowed to register.
    pub allowed_isolation: Vec<ProviderIsolation>,
    /// Optional implementation lock; locked registries only admit
    /// descriptors whose lock matches this identity.
    pub lock: Option<ContentId>,
}

impl RegistryConfig {
    /// Only in-process static providers.
    #[must_use]
    pub fn static_only() -> Self {
        Self {
            allowed_isolation: vec![ProviderIsolation::Static],
            lock: None,
        }
    }

    /// Default policy: exclude remote providers.
    #[must_use]
    pub fn no_remote() -> Self {
        Self {
            allowed_isolation: vec![
                ProviderIsolation::Static,
                ProviderIsolation::Sandboxed,
                ProviderIsolation::ExternalProcess,
            ],
            lock: None,
        }
    }

    /// Whether an isolation kind is permitted.
    #[must_use]
    pub fn allows(&self, isolation: ProviderIsolation) -> bool {
        self.allowed_isolation.contains(&isolation)
    }
}

/// Registry refusal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegistryError {
    /// Stable code (`E-PROV-510`/`E-PROV-511`).
    pub code: &'static str,
    /// Message.
    pub message: String,
}

/// Deterministic provider registry.
#[derive(Clone, Debug)]
pub struct ProviderRegistry {
    descriptors: BTreeMap<String, (ProviderIsolation, CapabilityTable)>,
    config: RegistryConfig,
}

impl ProviderRegistry {
    /// Empty registry under the given policy.
    #[must_use]
    pub fn new(config: RegistryConfig) -> Self {
        Self {
            descriptors: BTreeMap::new(),
            config,
        }
    }

    /// Registers a descriptor under an isolation claim, applying policy
    /// and lock checks.
    pub fn register(
        &mut self,
        id: &str,
        isolation: ProviderIsolation,
        table: CapabilityTable,
    ) -> Result<(), RegistryError> {
        if !self.config.allows(isolation) {
            return Err(RegistryError {
                code: "E-PROV-510",
                message: format!(
                    "registration `{id}` denied by policy: isolation `{}` not allowed",
                    isolation.name()
                ),
            });
        }
        if let Some(expected) = &self.config.lock {
            if !matches!(table.lock, ProviderLock::Locked(ref actual) if actual == expected) {
                return Err(RegistryError {
                    code: "E-PROV-511",
                    message: format!(
                        "registration `{id}` denied: implementation lock does not match `{}`",
                        expected.0
                    ),
                });
            }
        }
        if !table.validate().is_empty() {
            return Err(RegistryError {
                code: "E-PROV-501",
                message: format!("registration `{id}` rejected: descriptor invalid"),
            });
        }
        self.descriptors.insert(id.to_string(), (isolation, table));
        Ok(())
    }

    /// Lookup by identifier.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&CapabilityTable> {
        self.descriptors.get(id).map(|(_, table)| table)
    }

    /// Isolation of a registered provider.
    #[must_use]
    pub fn isolation_of(&self, id: &str) -> Option<ProviderIsolation> {
        self.descriptors.get(id).map(|(isolation, _)| *isolation)
    }

    /// Registered ids in deterministic order.
    #[must_use]
    pub fn ids(&self) -> Vec<String> {
        self.descriptors.keys().cloned().collect()
    }

    /// Number of registered providers.
    #[must_use]
    pub fn len(&self) -> usize {
        self.descriptors.len()
    }

    /// Whether the registry is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.descriptors.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::descriptor::{CapabilitySpec, RepresentationSpec};

    fn static_table() -> CapabilityTable {
        CapabilityTable {
            capabilities: vec![CapabilitySpec {
                name: "evaluate.strict-f64".into(),
                semantic_subset: "strict-f64".into(),
                representations: vec![RepresentationSpec {
                    name: "f64".into(),
                    exact_relation: "bit-identical".into(),
                    encode_cost: 0,
                }],
                exactness: vec!["exact".into()],
                failure_modes: vec![],
                checker_bindings: vec!["sir-checker.v1".into()],
            }],
            isolation: ProviderIsolation::Static,
            lock: ProviderLock::Unlocked,
            maximum_evidence: emath_ir::EvidenceLevel::E1,
            deterministic: true,
        }
    }

    #[test]
    fn remote_provider_is_denied_by_policy() {
        let mut registry = ProviderRegistry::new(RegistryConfig::no_remote());
        let error = registry
            .register("remote.vendor", ProviderIsolation::Remote, static_table())
            .unwrap_err();
        assert_eq!(error.code, "E-PROV-510");
        assert!(registry.is_empty());
    }

    #[test]
    fn remote_allowed_when_policy_says_so() {
        let mut config = RegistryConfig::no_remote();
        config.allowed_isolation.push(ProviderIsolation::Remote);
        let mut registry = ProviderRegistry::new(config);
        registry
            .register("remote.vendor", ProviderIsolation::Remote, static_table())
            .unwrap();
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn locked_registry_rejects_lock_mismatch() {
        let mut config = RegistryConfig::static_only();
        config.lock = Some(ContentId("fnv1a64:aaaaaaaaaaaaaaaa".into()));
        let mut registry = ProviderRegistry::new(config);
        let mut table = static_table();
        table.lock = ProviderLock::Locked(ContentId("fnv1a64:bbbbbbbbbbbbbbbb".into()));
        let error = registry
            .register("locked.provider", ProviderIsolation::Static, table)
            .unwrap_err();
        assert_eq!(error.code, "E-PROV-511");
    }

    #[test]
    fn invalid_descriptor_is_rejected() {
        let mut registry = ProviderRegistry::new(RegistryConfig::static_only());
        let mut table = static_table();
        table.capabilities[0].representations.clear();
        let error = registry
            .register("bad.descriptor", ProviderIsolation::Static, table)
            .unwrap_err();
        assert_eq!(error.code, "E-PROV-501");
    }

    #[test]
    fn lookup_and_order_are_deterministic() {
        let mut registry = ProviderRegistry::new(RegistryConfig::static_only());
        registry
            .register("beta", ProviderIsolation::Static, static_table())
            .unwrap();
        registry
            .register("alpha", ProviderIsolation::Static, static_table())
            .unwrap();
        assert_eq!(registry.ids(), ["alpha", "beta"]);
        assert!(registry.get("alpha").is_some());
        assert_eq!(
            registry.isolation_of("beta"),
            Some(ProviderIsolation::Static)
        );
    }
}
