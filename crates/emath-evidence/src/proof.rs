//!: proof-provider path.
//!
//! Theorem-proving kernels (`lean-4`, `frankenlean` or other backends) plug in
//! as *optional* E3 producers/checkers. Ordinary compilation never
//! requires one: with no provider configured, the seam returns a typed
//! refusal (`E-EVID-506`) and the claim's evidence stays diagnostic.
//!
//! Stable codes:
//! - `E-EVID-506` proof provider unavailable (optional path refusal).

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

/// Kernel seam: a proof back end may implement this to act as an
/// optional formal-proof producer/checker.
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

/// Optional seam entry point. Without a configured provider the call
/// refuses with `E-EVID-506`: theorem proving is never mandatory for
/// ordinary compilation and the evidence stays diagnostic.
pub fn verify_proof_optional(
    provider: Option<&dyn ProofProvider>,
    record: &EvidenceRecord,
    certificate: &[u8],
) -> Result<ProofVerdict, EvidenceError> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests_support::sample_record;

    /// Minimal deterministic kernel for tests: admits a non-empty
    /// certificate for correctness claims, refuses empty certificates,
    /// and declines any other class.
    struct FrankenLeanLite;

    impl ProofProvider for FrankenLeanLite {
        fn kernel_name(&self) -> &'static str {
            "frankenlean-lite"
        }

        fn verify(
            &self,
            record: &EvidenceRecord,
            certificate: &[u8],
        ) -> Result<ProofVerdict, EvidenceError> {
            if record.claim.class != "correctness" {
                return Ok(ProofVerdict {
                    kind: ProofVerdictKind::NotCovered,
                    certificate_id: None,
                });
            }
            if certificate.is_empty() {
                return Ok(ProofVerdict {
                    kind: ProofVerdictKind::Refusal,
                    certificate_id: None,
                });
            }
            Ok(ProofVerdict {
                kind: ProofVerdictKind::Admission,
                certificate_id: Some("frankenlean-lite:cert-1".into()),
            })
        }
    }

    #[test]
    fn missing_provider_is_a_typed_optional_refusal() {
        let record = sample_record();
        let error = verify_proof_optional(None, &record, b"certificate").unwrap_err();
        assert_eq!(error.code, "E-EVID-506");
        assert!(error.message.contains("optional"));
    }

    #[test]
    fn configured_kernel_admits_and_refuses_deterministically() {
        let kernel = FrankenLeanLite;
        let record = sample_record();
        let admitted = verify_proof_optional(Some(&kernel), &record, b"steps").unwrap();
        assert_eq!(admitted.kind, ProofVerdictKind::Admission);
        assert_eq!(
            admitted.certificate_id.as_deref(),
            Some("frankenlean-lite:cert-1")
        );
        let refused = verify_proof_optional(Some(&kernel), &record, b"").unwrap();
        assert_eq!(refused.kind, ProofVerdictKind::Refusal);

        let mut non_correctness = sample_record();
        non_correctness.claim.class = "performance".into();
        let not_covered = verify_proof_optional(Some(&kernel), &non_correctness, b"steps").unwrap();
        assert_eq!(not_covered.kind, ProofVerdictKind::NotCovered);
    }
}
