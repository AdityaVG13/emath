//! Certify-the-certifier corpus.
//!
//! A fixed corpus of known-unsound certifier outputs (optimizations,
//! estimators, proofs, translations) must be rejected by the admission
//! gate. The corpus doubles as the regression oracle: any future
//! certifier change that admits one of these patterns is itself
//! unsound.
//!
//! Stable codes:
//! - `E-EVID-507` unsound certifier output rejected by the corpus gate.

use crate::EvidenceError;
use crate::registry::CertificateKind;

/// A known-unsound certifier output, with the hole it hides.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnsoundFixture {
    /// Corpus key (stable, never renamed).
    pub pattern: &'static str,
    /// Certificate kind the pattern would claim.
    pub kind: CertificateKind,
    /// Why the pattern is unsound.
    pub hole: &'static str,
}

/// The certify-the-certifier corpus.
pub const CERTIFY_THE_CERTIFIER: [UnsoundFixture; 7] = [
    UnsoundFixture {
        pattern: "interval-under-approximation",
        kind: CertificateKind::Interval,
        hole: "enclosure computed on rounded-lower inputs only",
    },
    UnsoundFixture {
        pattern: "overflow-assumption-elision",
        kind: CertificateKind::Optimization,
        hole: "no-overflow assumption elided from the claim",
    },
    UnsoundFixture {
        pattern: "taylor-remainder-dropped",
        kind: CertificateKind::Residual,
        hole: "remainder term dropped without a bound",
    },
    UnsoundFixture {
        pattern: "zero-divisor-masked",
        kind: CertificateKind::Rewrite,
        hole: "rewrite cancels a divisor that can be zero",
    },
    UnsoundFixture {
        pattern: "unstated-axiom-step",
        kind: CertificateKind::Proof,
        hole: "proof step depends on an unstated axiom",
    },
    UnsoundFixture {
        pattern: "float-reassociation-translation",
        kind: CertificateKind::Translation,
        hole: "translation reassociates floats under strict-f64",
    },
    UnsoundFixture {
        pattern: "witness-outside-domain",
        kind: CertificateKind::Witness,
        hole: "witness does not lie in the declared domain",
    },
];

/// Admission gate over certifier output: a certificate carrying any
/// corpus pattern is refused (`E-EVID-507`) with the pattern's hole;
/// output carrying none of them passes the gate (it is not *provably*
/// unsound by the corpus).
pub fn reject_unsound_certifier_output(certificate: &[u8]) -> Result<(), EvidenceError> {
    // Binary or non-UTF-8 payloads are not certificates: the earlier
    // empty-fallback let them pass the corpus unchecked.
    let text = std::str::from_utf8(certificate).map_err(|_| {
        EvidenceError::new(
            "E-EVID-507",
            "certifier output is not valid UTF-8; refusing as untrusted payload",
        )
    })?;
    for fixture in &CERTIFY_THE_CERTIFIER {
        if text.contains(fixture.pattern) {
            return Err(EvidenceError::new(
                "E-EVID-507",
                format!(
                    "unsound certifier pattern `{}` ({} certificate): {}",
                    fixture.pattern,
                    fixture.kind.as_str(),
                    fixture.hole
                ),
            ));
        }
    }
    Ok(())
}
