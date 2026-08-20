//! Independent artifact checking, translation validation, negative
//! controls and claim-language linting (Phase 8).
//!
//! The checker never invokes generator internals: authority is rebuilt
//! exclusively from the retained artifact (manifest, source map, plan,
//! evidence bundle, file content, provider locks) and content identity.
//!
//! Stable codes (`E-EVID-*`, evidence/checker area):
//! - `E-EVID-101` content-identity mismatch (bootstrap hash);
//! - `E-EVID-102` artifact identity does not recompute;
//! - `E-EVID-103` evidence/goal scope mismatch (wrong goal or source);
//! - `E-EVID-104` stale certificate (freshness window passed);
//! - `E-EVID-105` incomplete artifact (required path missing);
//! - `E-EVID-106` unsupported claim class;
//! - `E-EVID-107` resolved claim without a checker;
//! - `E-EVID-108` manifest schema mismatch;
//! - `E-EVID-109` file-inventory mismatch (undeclared/missing entry);
//! - `E-EVID-110` source-map mismatch;
//! - `E-EVID-111` provider lock mismatch;
//! - `E-EVID-112` source map does not reference the manifest's package;
//! - `E-EVID-113` required/declared artifact path is a symlink;
//! - `E-EVID-114` artifact document or declared file is not valid UTF-8;
//! - `E-EVID-201` claim language stronger than the available evidence;
//! - `E-EVID-301` translation mismatch (no equivalence witness);
//! - `E-EVID-302` witness cannot be independently verified.

#![forbid(unsafe_code)]

pub mod artifact_check;
pub mod claimlint;
pub mod negative;
pub mod translation;

pub use artifact_check::{
    ArtifactCheckConfig, ArtifactCheckIssue, ArtifactCheckReport, ArtifactInput,
    ProviderLockRecord, artifact_input_from_dir, check_artifact, check_artifact_dir,
};
pub use claimlint::{ClaimLinter, LintIssue, lint_claims};
pub use negative::{
    ControlRun, NegativeControl, NegativeControlKind, run_negative_controls, run_standard_battery,
    seed_incomplete, seed_stale, seed_tampered, seed_unsupported, seed_wrong_derivative,
    seed_wrong_goal,
};
pub use translation::{
    EquivalenceWitness, TranslationRelation, TranslationSample, check_witness, validate_translation,
};

use emath_core::ContentId;

/// Shared checker failure with a stable code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckerError {
    /// Stable code (`E-EVID-*`).
    pub code: &'static str,
    /// Message.
    pub message: String,
}

impl CheckerError {
    #[must_use]
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for CheckerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for CheckerError {}

/// Deterministic FNV-1a64 identity helper shared by the checker modules.
#[must_use]
pub(crate) fn identity_of(text: &str) -> ContentId {
    ContentId(format!(
        "fnv1a64:{:016x}",
        emath_core::fnv1a64_bytes(text.as_bytes())
    ))
}
