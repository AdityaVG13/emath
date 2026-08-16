//!: certificate registry.
//!
//! Versioned checker contracts for the seven certificate kinds
//! (interval, witness, residual, optimization, rewrite, proof,
//! translation). A contract says which claim classes a checker of
//! that kind admits, which artifacts it consumes and which certificate
//! it emits. The registry refuses unknown kinds (`E-EVID-401`),
//! duplicate versioned contracts (`E-EVID-402`) and contracts that do
//! not admit a claim class (`E-EVID-403`).

use std::collections::BTreeMap;

use crate::EvidenceError;

/// Certificate kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CertificateKind {
    /// Interval enclosure certificate.
    Interval,
    /// Witness certificate.
    Witness,
    /// Residual certificate.
    Residual,
    /// Optimization certificate.
    Optimization,
    /// Rewrite equivalence certificate.
    Rewrite,
    /// Machine-checkable proof certificate.
    Proof,
    /// Translation equivalence certificate.
    Translation,
}

impl CertificateKind {
    /// Stable kind token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Interval => "interval",
            Self::Witness => "witness",
            Self::Residual => "residual",
            Self::Optimization => "optimization",
            Self::Rewrite => "rewrite",
            Self::Proof => "proof",
            Self::Translation => "translation",
        }
    }

    /// All kinds, in stable order.
    #[must_use]
    pub const fn all() -> [Self; 7] {
        [
            Self::Interval,
            Self::Witness,
            Self::Residual,
            Self::Optimization,
            Self::Rewrite,
            Self::Proof,
            Self::Translation,
        ]
    }
}

/// Checker contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckerContract {
    /// Certificate kind this checker produces.
    pub kind: CertificateKind,
    /// Semantic version of the contract.
    pub version: String,
    /// Checker id implementing the contract.
    pub checker_id: String,
    /// Claim classes the checker admits.
    pub admits: Vec<String>,
    /// Artifact paths the checker consumes.
    pub input_artifacts: Vec<String>,
    /// Certificate path the checker emits.
    pub output_certificate: String,
    /// Whether the checker must be deterministic.
    pub determinism_required: bool,
}

/// Certificate registry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CertificateRegistry {
    contracts: BTreeMap<String, CheckerContract>,
}

impl CertificateRegistry {
    /// Registers a versioned contract; a second contract for the same
    /// `(kind, version)` is refused (`E-EVID-402`).
    pub fn register(&mut self, contract: CheckerContract) -> Result<(), EvidenceError> {
        let key = Self::key(contract.kind, &contract.version);
        if self.contracts.contains_key(&key) {
            return Err(EvidenceError::new(
                "E-EVID-402",
                format!(
                    "contract already registered for {} v{}",
                    contract.kind.as_str(),
                    contract.version
                ),
            ));
        }
        self.contracts.insert(key, contract);
        Ok(())
    }

    /// Looks up a contract; missing entries are refused with
    /// `E-EVID-401` (unknown kind/version tuple).
    pub fn lookup(
        &self,
        kind: CertificateKind,
        version: &str,
    ) -> Result<&CheckerContract, EvidenceError> {
        self.contracts
            .get(&Self::key(kind, version))
            .ok_or_else(|| {
                EvidenceError::new(
                    "E-EVID-401",
                    format!("unknown certificate contract {} v{version}", kind.as_str()),
                )
            })
    }

    /// Whether a registered contract admits the claim class;
    /// unregistered contracts are refused (`E-EVID-401`).
    pub fn admits(
        &self,
        kind: CertificateKind,
        version: &str,
        claim_class: &str,
    ) -> Result<bool, EvidenceError> {
        let contract = self.lookup(kind, version)?;
        Ok(contract
            .admits
            .iter()
            .any(|admitted| admitted == claim_class))
    }

    /// Deterministic registry token.
    #[must_use]
    pub fn canonical(&self) -> String {
        let rows: Vec<String> = self
            .contracts
            .iter()
            .map(|(key, contract)| {
                format!(
                    "{}:{}({})",
                    key,
                    contract.checker_id,
                    contract.admits.join(",")
                )
            })
            .collect();
        rows.join(";")
    }

    fn key(kind: CertificateKind, version: &str) -> String {
        format!("{}:{version}", kind.as_str())
    }
}

/// Free-function accessors matching the module re-exports.
pub fn register_contract(
    registry: &mut CertificateRegistry,
    contract: CheckerContract,
) -> Result<(), EvidenceError> {
    registry.register(contract)
}

pub fn lookup_contract<'a>(
    registry: &'a CertificateRegistry,
    kind: CertificateKind,
    version: &str,
) -> Result<&'a CheckerContract, EvidenceError> {
    registry.lookup(kind, version)
}

pub fn admits_claim_class(
    registry: &CertificateRegistry,
    kind: CertificateKind,
    version: &str,
    claim_class: &str,
) -> Result<bool, EvidenceError> {
    registry.admits(kind, version, claim_class)
}
