//! Deterministic world versioning (spec 12): a world invalidated by
//! future examples becomes a new version or a semantic delta, never a
//! silent redefinition.

use emath_world_ir::fnv1a64;

/// Seed mixed into every deterministic version stamp.
pub const VERSION_SEED: u64 = 0x5eed_0007;

/// A deterministic world version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldVersion {
    /// Stable label, e.g. `v1`.
    pub label: String,
    /// FNV-1a64 over (seed, label, claim).
    pub stamp: u64,
}

impl WorldVersion {
    /// Stamps a version deterministically from its claim.
    #[must_use]
    pub fn stamped(label: impl Into<String>, claim: &str) -> Self {
        let label = label.into();
        let stamp = fnv1a64(format!("{VERSION_SEED}:{label}:{claim}").as_bytes());
        Self { label, stamp }
    }
}
