//! Proof-provider path.
//!
//! Theorem-proving kernels plug in as *optional* E3 producers/checkers.
//! Ordinary compilation never requires one; with no provider configured
//! the seam refuses (`E-EVID-506`) and evidence stays diagnostic.

use crate::{EvidenceError, EvidenceRecord};

/// Machine-checked verdict over a certificate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProofVerdictKind {
    /// The kernel admitted the certificate for the claim.
    Admission,
    /// The kernel refused the certificate.
    Refusal,
    /// The kernel does not cover this claim class.
    NotCovered,
}

impl ProofVerdictKind {
    /// Stable verdict token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Admission => "admission",
            Self::Refusal => "refusal",
            Self::NotCovered => "not-covered",
        }
    }
}

/// Result of a kernel check.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProofVerdict {
    /// Kernel verdict.
    pub kind: ProofVerdictKind,
    /// Certificate id produced by the kernel on admission.
    pub certificate_id: Option<String>,
}

/// Kernel seam for an optional formal-proof producer/checker.
pub trait ProofProvider {
    /// Kernel name (stable token, e.g. `lean-4`).
    fn kernel_name(&self) -> &'static str;

    /// Machine-checks `certificate` against the record's claim.
    fn verify(
        &self,
        record: &EvidenceRecord,
        certificate: &[u8],
    ) -> Result<ProofVerdict, EvidenceError>;
}

/// Optional seam entry point; without a configured provider the call
/// refuses (`E-EVID-506`), evidence stays diagnostic.
pub fn verify_proof_optional(
    provider: Option<&dyn ProofProvider>,
    record: &EvidenceRecord,
    certificate: &[u8],
) -> Result<ProofVerdict, EvidenceError> {
    // Every claimed proof passes the certify-the-certifier corpus (and the UTF-8    // gate)
    // before any kernel sees it ("E-EVID-507").
    crate::certify::reject_unsound_certifier_output(certificate)?;
    match provider {
        Some(kernel) => kernel.verify(record, certificate),
        None => Err(EvidenceError::new(
            "E-EVID-506",
            format!(
                "no proof kernel configured for claim {}; theorem proving is optional and the evidence remains diagnostic",
                record.claim.id
            ),
        )),
    }
}
