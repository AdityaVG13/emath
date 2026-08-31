//! Minimal fork seam.
//!
//! The adapter exposes a stable adapter-facing API and never edits Dew
//! expression internals. Every patch to the fork is categorized in the
//! patch ledger; this adapter currently requires zero patches.

/// Provider version proven by the seam.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderVersion {
    /// Upstream version string from `forks/UPSTREAM_LOCK.json`.
    pub upstream: String,
    /// Adapter protocol version.
    pub adapter_protocol: String,
}

impl ProviderVersion {
    /// Whether the upstream version is compatible with the required
    /// range (same major segment).
    #[must_use]
    pub fn compatible_with(&self, required_major: u64) -> bool {
        self.upstream
            .split('.')
            .next()
            .and_then(|part| part.parse::<u64>().ok())
            .is_some_and(|major| major == required_major)
    }
}

/// Category of a fork patch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PatchOutcome {
    /// Patch merged upstream or superseded.
    Superseded,
    /// Patch retained and needed by the adapter.
    Required,
    /// Patch was reverted/adapter worked around it.
    WorkedAround,
}

/// One categorized fork patch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PatchLedger {
    /// Patch identifier.
    pub patch: String,
    /// Category.
    pub outcome: PatchOutcome,
    /// Motivation.
    pub motivation: String,
}

/// Versioned adapter seam (mirrors the Rumoca adapter pattern).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdapterSeam {
    pub version: ProviderVersion,
    pub patches: Vec<PatchLedger>,
}

impl AdapterSeam {
    /// The locked upstream commit for `dew` (the `repositories[].commit`
    /// row in `forks/UPSTREAM_LOCK.json`). The seam binds to the locked
    /// revision, never a floating version string (conformance pin
    /// register, `emath-conform-pin-register-1iip`).
    pub const LOCKED_UPSTREAM_COMMIT: &'static str =
        "0dd40dd1c374cb05d26e0d5c2b0746a217bf93ab";

    /// The seam for the current fork state: upstream locked version,
    /// adapter-only API, zero fork patches.
    #[must_use]
    pub fn current() -> Self {
        Self {
            version: ProviderVersion {
                upstream: format!("dew-0.1.0+{}", Self::LOCKED_UPSTREAM_COMMIT),
                adapter_protocol: "dew-adapter-1".into(),
            },
            patches: Vec::new(),
        }
    }
}

/// Seam negotiation failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SeamError {
    /// Upstream major version drifted.
    VersionDrift { found: String, required_major: u64 },
    /// The fork carries mandatory uncategorized patches.
    UncategorizedPatches { count: usize },
}

impl SeamError {
    /// Stable code (`E-PROV-001` version drift).
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::VersionDrift { .. } => "E-PROV-001",
            Self::UncategorizedPatches { .. } => "E-PROV-002",
        }
    }
}

/// Negotiates the seam: version compatibility and patch ledger hygiene.
pub fn negotiate(seam: &AdapterSeam, required_major: u64) -> Result<(), SeamError> {
    if !seam.version.compatible_with(required_major) {
        return Err(SeamError::VersionDrift {
            found: seam.version.upstream.clone(),
            required_major,
        });
    }
    if seam
        .patches
        .iter()
        .any(|patch| patch.outcome == PatchOutcome::Required)
    {
        let count = seam
            .patches
            .iter()
            .filter(|patch| patch.outcome == PatchOutcome::Required)
            .count();
        return Err(SeamError::UncategorizedPatches { count });
    }
    Ok(())
}
