//! Negative-control framework.
//!
//! Seeds tampered, stale, wrong-goal, incomplete and unsupported
//! artifacts for every checker. A control that the checker admits is an
//! escaped defect; the framework reports escapes without masking them.

use crate::ArtifactCheckConfig;
use crate::artifact_check::{ArtifactInput, check_artifact};

/// Kind of seeded negative control.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NegativeControlKind {
    /// One file's content was altered.
    TamperedContent,
    /// A certificate freshness window was left in the past.
    StaleCertificate,
    /// Evidence scoped to a different goal/source.
    WrongGoal,
    /// A required artifact path was removed.
    IncompleteArtifact,
    /// A resolved claim uses an unsupported claim class.
    UnsupportedClaim,
}

impl NegativeControlKind {
    /// Stable kind token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TamperedContent => "tampered-content",
            Self::StaleCertificate => "stale-certificate",
            Self::WrongGoal => "wrong-goal",
            Self::IncompleteArtifact => "incomplete-artifact",
            Self::UnsupportedClaim => "unsupported-claim",
        }
    }

    /// The stable code the independent checker must refuse with.
    #[must_use]
    pub const fn expected_code(self) -> &'static str {
        match self {
            Self::TamperedContent => "E-EVID-101",
            Self::StaleCertificate => "E-EVID-104",
            Self::WrongGoal => "E-EVID-103",
            Self::IncompleteArtifact => "E-EVID-105",
            Self::UnsupportedClaim => "E-EVID-106",
        }
    }
}

/// One seeded negative control.
#[derive(Clone, Debug, PartialEq)]
pub struct NegativeControl {
    /// Control id.
    pub id: String,
    /// Control kind.
    pub kind: NegativeControlKind,
    /// Seeded (invalid) artifact.
    pub artifact: ArtifactInput,
}

/// Result of running the negative-control framework.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ControlRun {
    /// Controls correctly refused with the expected code.
    pub refused: Vec<String>,
    /// Controls that escaped (not refused with the expected code).
    pub escaped: Vec<(String, String)>,
}

impl ControlRun {
    /// Whether every control was refused as expected.
    #[must_use]
    pub fn all_refused(&self) -> bool {
        self.escaped.is_empty()
    }
}

/// Runs every control through the independent checker; a control that is
/// refused **with its expected code** counts as refused, anything else is
/// an escape.
#[must_use]
pub fn run_negative_controls(
    controls: &[NegativeControl],
    config: &ArtifactCheckConfig,
) -> ControlRun {
    let mut run = ControlRun::default();
    for control in controls {
        let report = check_artifact(&control.artifact, config);
        let expected = control.kind.expected_code();
        if report.issues.iter().any(|found| found.code == expected) {
            run.refused.push(control.id.clone());
        } else {
            let observed = report
                .issues
                .first()
                .map_or_else(|| "admitted".to_string(), |found| found.code.to_string());
            run.escaped.push((control.id.clone(), observed));
        }
    }
    run
}

/// Seeds the given artifacts.
/// - altered: flips one byte of `src/lib.rs`.
/// - stale: moves every certificate freshness window into the past.
/// - wrong-goal: re-scopes the evidence bundle to another goal.
/// - incomplete: removes `src/lib.rs`.
/// - unsupported: re-classifies the first resolved claim.
pub fn seed_tampered(input: &ArtifactInput) -> NegativeControl {
    let mut artifact = input.clone();
    if let Some(content) = artifact.files.get_mut("src/lib.rs") {
        let mut bytes = content.clone().into_bytes();
        if let Some(byte) = bytes.first_mut() {
            *byte ^= 0x01;
        }
        *content = String::from_utf8_lossy(&bytes).into_owned();
    }
    NegativeControl {
        id: "tampered-content".into(),
        kind: NegativeControlKind::TamperedContent,
        artifact,
    }
}

/// Seeds a stale-certificate control (freshness left in the past).
pub fn seed_stale(input: &ArtifactInput) -> NegativeControl {
    let mut artifact = input.clone();
    for claim in &mut artifact.evidence.claims {
        claim.fresh_until = Some("2001-01-01T00:00:00Z".into());
    }
    NegativeControl {
        id: "stale-certificate".into(),
        kind: NegativeControlKind::StaleCertificate,
        artifact,
    }
}

/// Seeds a wrong-goal control (evidence from another experiment).
pub fn seed_wrong_goal(input: &ArtifactInput) -> NegativeControl {
    let mut artifact = input.clone();
    artifact.evidence.source_package = emath_core::ContentId("goal-other".into());
    NegativeControl {
        id: "wrong-goal".into(),
        kind: NegativeControlKind::WrongGoal,
        artifact,
    }
}

/// Seeds an incomplete-artifact control (required path removed).
pub fn seed_incomplete(input: &ArtifactInput) -> NegativeControl {
    let mut artifact = input.clone();
    artifact.files.remove("src/lib.rs");
    NegativeControl {
        id: "incomplete-artifact".into(),
        kind: NegativeControlKind::IncompleteArtifact,
        artifact,
    }
}

/// Seeds an unsupported-claim control (unknown claim class).
pub fn seed_unsupported(input: &ArtifactInput) -> NegativeControl {
    let mut artifact = input.clone();
    if let Some(claim) = artifact.evidence.claims.first_mut() {
        claim.class = "hypothesis".into();
    }
    NegativeControl {
        id: "unsupported-claim".into(),
        kind: NegativeControlKind::UnsupportedClaim,
        artifact,
    }
}

/// Convenience: run the full standard control battery over one artifact.
#[must_use]
pub fn run_standard_battery(input: &ArtifactInput) -> ControlRun {
    let controls = vec![
        seed_tampered(input),
        seed_stale(input),
        seed_wrong_goal(input),
        seed_incomplete(input),
        seed_unsupported(input),
    ];
    run_negative_controls(
        &controls,
        &ArtifactCheckConfig {
            supported_claim_classes: vec!["correctness".into()],
        },
    )
}
