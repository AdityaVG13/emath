//! Feature-scoped language publication gates.

use emath_artifact::{
    AuthorityEntry, AuthorityError, AuthorityEvidence, AuthorityLock, AuthorityReceipt,
    AuthorityState,
};
use emath_core::FeatureId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublicationMode {
    Framework,
    CandidateImage,
    StableLanguage,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PublicationEvidence {
    pub schemas_valid: bool,
    pub reference_vectors_valid: bool,
    pub deterministic_image: bool,
    pub unrealized_coverage_explicit: bool,
    pub projections_complete: bool,
    pub live_adapter: bool,
    pub unique_authority: bool,
    pub no_blocking_holes: bool,
    pub migrations_valid: bool,
    pub independent_conformance: bool,
    pub generated_views_fresh: bool,
    pub authorized_semantic_change: bool,
    pub conformance: Vec<String>,
    pub generated_views: Vec<String>,
    pub rollback: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PublicationError {
    MissingGate(&'static str),
    Authority(AuthorityError),
}

pub fn publish_feature(
    mode: PublicationMode,
    lock: &mut AuthorityLock,
    feature: &FeatureId,
    next: AuthorityEntry,
    evidence: PublicationEvidence,
) -> Result<AuthorityReceipt, PublicationError> {
    require(evidence.schemas_valid, "schemas")?;
    require(evidence.reference_vectors_valid, "reference-vectors")?;
    if matches!(
        mode,
        PublicationMode::CandidateImage | PublicationMode::StableLanguage
    ) {
        require(evidence.deterministic_image, "deterministic-image")?;
        require(evidence.unrealized_coverage_explicit, "unrealized-coverage")?;
    }
    if mode == PublicationMode::StableLanguage {
        require(evidence.projections_complete, "projection-closure")?;
        require(evidence.live_adapter, "live-adapter")?;
        require(evidence.unique_authority, "unique-authority")?;
        require(evidence.no_blocking_holes, "blocking-spec-hole")?;
        require(evidence.migrations_valid, "migration")?;
        require(evidence.independent_conformance, "independent-conformance")?;
        require(evidence.generated_views_fresh, "generated-views")?;
        require(
            evidence.authorized_semantic_change,
            "authorized-semantic-change",
        )?;
    }
    lock.transition(
        feature,
        next,
        AuthorityEvidence {
            conformance: evidence.conformance,
            generated_views: evidence.generated_views,
            rollback: evidence.rollback,
        },
    )
    .map_err(PublicationError::Authority)
}

fn require(value: bool, gate: &'static str) -> Result<(), PublicationError> {
    value
        .then_some(())
        .ok_or(PublicationError::MissingGate(gate))
}

#[must_use]
pub fn authority_status(lock: &AuthorityLock) -> Vec<(FeatureId, AuthorityState)> {
    lock.entries
        .iter()
        .map(|(id, entry)| (id.clone(), entry.state))
        .collect()
}
