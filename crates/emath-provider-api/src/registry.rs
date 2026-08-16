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
        // Policy is enforced on the table-advertised isolation, never on a
        // caller-supplied argument: a table cannot sneak remote behind a
        // static-only policy by claiming otherwise.
        if isolation != table.isolation {
            return Err(RegistryError {
            code: "E-PROV-510",
            message: format!("registration `{}` denied: isolation claim `{}` contradicts advertised table isolation `{}`", id, isolation.name(), table.isolation.name()),
        });
        }
        if !self.config.allows(table.isolation) {
            return Err(RegistryError {
                code: "E-PROV-510",
                message: format!(
                    "registration `{}` denied by policy: advertised isolation `{}` not allowed",
                    id,
                    table.isolation.name()
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
        self.descriptors
            .insert(id.to_string(), (table.isolation, table));
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
