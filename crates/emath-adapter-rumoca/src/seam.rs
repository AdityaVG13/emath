//! Versioned provider seam.
//!
//! Adapters are versioned and identity-checked before their output is
//! trusted. Provider output is untrusted until checked; drift or identity
//! mismatch surfaces as a typed error, never a silent fallback.

use crate::structural::{ModelIssue, StructuralModel};

/// Semantic version of a provider adapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProviderVersion {
    /// Major version.
    pub major: u32,
    /// Minor version.
    pub minor: u32,
    /// Patch version.
    pub patch: u32,
}

impl ProviderVersion {
    /// Renders `major.minor.patch`.
    #[must_use]
    pub fn render(&self) -> String {
        format!("{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Versioned, identity-checked adapter seam for a provider.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdapterSeam {
    /// Provider identity (e.g. "rumoca").
    pub provider: String,
    /// Adapter version.
    pub version: ProviderVersion,
    /// Advertised capability schema identity.
    pub capability: String,
    /// FNV-1a64 over the provider contract digest.
    pub content_identity: u64,
}

impl AdapterSeam {
    /// Verifies an observed seam against the expected (locked) seam.
    pub fn verify(&self, expected: &AdapterSeam) -> Result<(), SeamError> {
        if self.provider != expected.provider || self.capability != expected.capability {
            return Err(SeamError::IdentityMismatch {
                provider: self.provider.clone(),
                expected_provider: expected.provider.clone(),
                capability: self.capability.clone(),
                expected_capability: expected.capability.clone(),
            });
        }
        if self.version != expected.version || self.content_identity != expected.content_identity {
            return Err(SeamError::VersionDrift {
                provider: self.provider.clone(),
                observed: self.version.render(),
                locked: expected.version.render(),
                observed_identity: self.content_identity,
                locked_identity: expected.content_identity,
            });
        }
        Ok(())
    }

    /// Runs untrusted provider output through the neutral validation gate.
    #[must_use]
    pub fn gate_provider_output(&self, model: &StructuralModel) -> Vec<ModelIssue> {
        model.validate()
    }
}

/// Seam verification failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SeamError {
    /// Provider or capability identity differs from the lock.
    IdentityMismatch {
        /// Observed provider.
        provider: String,
        /// Locked provider.
        expected_provider: String,
        /// Observed capability.
        capability: String,
        /// Locked capability.
        expected_capability: String,
    },
    /// Version or content identity drifted from the lock.
    VersionDrift {
        /// Provider identity.
        provider: String,
        /// Observed version.
        observed: String,
        /// Locked version.
        locked: String,
        /// Observed content identity.
        observed_identity: u64,
        /// Locked content identity.
        locked_identity: u64,
    },
}

impl SeamError {
    /// Stable diagnostic code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::IdentityMismatch { .. } => "E-PROV-402",
            Self::VersionDrift { .. } => "E-PROV-401",
        }
    }
}
