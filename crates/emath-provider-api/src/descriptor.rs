//! Provider descriptor schema.
//!
//! Capability vocabulary, semantic subsets, representations with costs,
//! evidence ceilings, permissions, targets, failure modes and checker
//! bindings. Descriptors self-validate with stable `E-PROV-501`.. codes and
//! carry a versioned canonical encoding.

use emath_core::ContentId;

/// How a provider executes relative to the host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderIsolation {
    /// In-process static component.
    Static,
    /// Sandboxed process.
    Sandboxed,
    /// External process.
    ExternalProcess,
    /// Remote service.
    Remote,
}

impl ProviderIsolation {
    /// Stable name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Static => "static",
            Self::Sandboxed => "sandboxed",
            Self::ExternalProcess => "external-process",
            Self::Remote => "remote",
        }
    }
}

/// Lock state of a registered provider implementation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProviderLock {
    /// Not locked.
    Unlocked,
    /// Locked to a pinned implementation identity.
    Locked(ContentId),
}

impl ProviderLock {
    /// Whether the lock matches an expected identity.
    #[must_use]
    pub fn admits(&self, expected: &ContentId) -> bool {
        match self {
            Self::Unlocked => true,
            Self::Locked(actual) => actual == expected,
        }
    }
}

/// One SIR representation a capability can produce/consume.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepresentationSpec {
    /// Representation name (e.g. `csc-matrix`, `degC`).
    pub name: String,
    /// Exactness relation to the SIR canonical form.
    pub exact_relation: String,
    /// Conversion cost (0 = identity).
    pub encode_cost: u8,
}

/// One capability in a provider's vocabulary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapabilitySpec {
    /// Capability name (e.g. `evaluate.strict-f64`).
    pub name: String,
    /// Semantic subset served.
    pub semantic_subset: String,
    /// Representation options.
    pub representations: Vec<RepresentationSpec>,
    /// Exactness tokens: `exact`, `bounded`, `estimate`.
    pub exactness: Vec<String>,
    /// Declared failure modes.
    pub failure_modes: Vec<String>,
    /// Checker bindings (evidence authorities).
    pub checker_bindings: Vec<String>,
}

impl CapabilitySpec {
    /// Whether the capability offers an exactness token.
    #[must_use]
    pub fn offers(&self, token: &str) -> bool {
        self.exactness.iter().any(|candidate| candidate == token)
    }
}

/// Schema problem found while validating a descriptor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DescriptorProblem {
    /// Stable code (`E-PROV-501`..`E-PROV-503`).
    pub code: &'static str,
    /// Message.
    pub message: String,
}

/// Descriptor schema extension: capability vocabulary, isolation and lock.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapabilityTable {
    /// Capability vocabulary (sorted by name in canonical form).
    pub capabilities: Vec<CapabilitySpec>,
    /// Execution isolation.
    pub isolation: ProviderIsolation,
    /// Implementation lock.
    pub lock: ProviderLock,
    /// Highest evidence level served.
    pub maximum_evidence: emath_ir::EvidenceLevel,
    /// Deterministic output guarantee.
    pub deterministic: bool,
}

impl Default for CapabilityTable {
    fn default() -> Self {
        Self {
            capabilities: vec![],
            isolation: ProviderIsolation::Static,
            lock: ProviderLock::Unlocked,
            maximum_evidence: emath_ir::EvidenceLevel::E0,
            deterministic: true,
        }
    }
}

impl CapabilityTable {
    /// Determinism guarantee accessor.
    #[must_use]
    pub fn deterministic(&self) -> bool {
        self.deterministic
    }
}

impl CapabilityTable {
    /// Validates the capability vocabulary.
    #[must_use]
    pub fn validate(&self) -> Vec<DescriptorProblem> {
        let mut problems = Vec::new();
        let mut names: Vec<&str> = self
            .capabilities
            .iter()
            .map(|cap| cap.name.as_str())
            .collect();
        names.sort_unstable();
        if names.windows(2).any(|pair| pair[0] == pair[1]) {
            problems.push(DescriptorProblem {
                code: "E-PROV-502",
                message: "duplicate capability name in descriptor".into(),
            });
        }
        for capability in &self.capabilities {
            if capability.representations.is_empty() {
                problems.push(DescriptorProblem {
                    code: "E-PROV-503",
                    message: format!(
                        "capability `{}` declares no representations",
                        capability.name
                    ),
                });
            }
            if capability.semantic_subset.is_empty() {
                problems.push(DescriptorProblem {
                    code: "E-PROV-503",
                    message: format!(
                        "capability `{}` declares no semantic subset",
                        capability.name
                    ),
                });
            }
        }
        problems
    }

    /// Versioned canonical encoding (`descriptor:`).
    ///
    /// Includes isolation, lock, evidence ceiling, and determinism so drift
    /// changes identity like `emath-plan::ProviderFingerprint` (cross-crate contract).
    #[must_use]
    pub fn canonical(&self) -> String {
        let mut capabilities: Vec<&CapabilitySpec> = self.capabilities.iter().collect();
        capabilities.sort_by(|left, right| left.name.cmp(&right.name));
        format!(
            "descriptor:{}:{}:ev{}:det{}:{}",
            self.isolation.name(),
            lock_token(&self.lock),
            self.maximum_evidence.as_str(),
            u8::from(self.deterministic),
            capabilities
                .iter()
                .map(|capability| capability_token(capability))
                .collect::<Vec<_>>()
                .join("|"),
        )
    }
}

/// Deterministic capability token.
#[must_use]
pub fn capability_token(capability: &CapabilitySpec) -> String {
    let mut representations: Vec<&RepresentationSpec> = capability.representations.iter().collect();
    representations.sort_by(|left, right| left.name.cmp(&right.name));
    format!(
        "{}@{}:{}:{}:{}",
        capability.name,
        capability.semantic_subset,
        capability.exactness.join("+"),
        representations
            .iter()
            .map(|representation| {
                format!("{}={}", representation.name, representation.encode_cost)
            })
            .collect::<Vec<_>>()
            .join(","),
        capability.checker_bindings.join("+"),
    )
}

/// Deterministic lock token.
#[must_use]
pub fn lock_token(lock: &ProviderLock) -> String {
    match lock {
        ProviderLock::Unlocked => "unlocked".to_string(),
        ProviderLock::Locked(identity) => format!("locked:{}", identity.0),
    }
}
