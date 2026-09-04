//! Atomic authority cutover for the fixed foundational Int/add slice.

use std::collections::BTreeSet;
use std::str::FromStr;

use emath_artifact::{AuthorityEntry, AuthorityLock, AuthorityState};
use emath_core::{FeatureId, SemanticHash};

use crate::{PublicationEvidence, PublicationMode, publish_feature};

pub const FIRST_CUTOVER_IDS: [&str; 18] = [
    "std.syntax.source",
    "std.syntax.declaration.generic",
    "std.syntax.section.generic",
    "std.section.inputs",
    "std.section.outputs",
    "std.section.definitions",
    "std.section.tests",
    "std.kind.function",
    "std.type.int",
    "std.symbol.math.add",
    "std.capability.math.add",
    "std.theory.additive_monoid",
    "std.instance.int.additive_monoid",
    "std.world.exact.int",
    "std.artifact.source",
    "std.artifact.value",
    "std.artifact.diagnostic",
    "std.diagnostic.exactness_loss",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CutoverError {
    WrongFeatureSet,
    MissingFeature(FeatureId),
    Publication(String),
}

pub fn activate_first_cutover(
    lock: &mut AuthorityLock,
    hashes: &[(FeatureId, SemanticHash)],
    evidence: &PublicationEvidence,
) -> Result<(), CutoverError> {
    let expected = FIRST_CUTOVER_IDS
        .iter()
        .map(|id| FeatureId::from_str(id).expect("fixed valid FeatureID"))
        .collect::<BTreeSet<_>>();
    let supplied = hashes
        .iter()
        .map(|(id, _)| id.clone())
        .collect::<BTreeSet<_>>();
    if supplied != expected {
        return Err(CutoverError::WrongFeatureSet);
    }
    let original = lock.clone();
    for (feature, hash) in hashes {
        let current = lock
            .entries
            .get(feature)
            .ok_or_else(|| CutoverError::MissingFeature(feature.clone()))?
            .state;
        let transitions: &[AuthorityState] = match current {
            AuthorityState::LegacyActive => &[
                AuthorityState::CapsuleCandidate,
                AuthorityState::LegacyActiveDualRun,
                AuthorityState::CapsuleActive,
            ],
            AuthorityState::CapsuleCandidate => &[
                AuthorityState::LegacyActiveDualRun,
                AuthorityState::CapsuleActive,
            ],
            AuthorityState::LegacyActiveDualRun => &[AuthorityState::CapsuleActive],
            AuthorityState::CapsuleActive => &[],
            _ => {
                *lock = original;
                return Err(CutoverError::Publication(format!(
                    "{} has invalid starting state",
                    feature
                )));
            }
        };
        for state in transitions {
            let source = if *state == AuthorityState::LegacyActiveDualRun {
                "legacy"
            } else {
                "capsule"
            };
            if let Err(error) = publish_feature(
                PublicationMode::StableLanguage,
                lock,
                feature,
                AuthorityEntry {
                    state: *state,
                    active_source: source.to_string(),
                    semantic_hash: hash.clone(),
                },
                evidence.clone(),
            ) {
                *lock = original;
                return Err(CutoverError::Publication(format!("{error:?}")));
            }
        }
    }
    Ok(())
}

pub fn rollback_feature(
    lock: &mut AuthorityLock,
    feature: &FeatureId,
    legacy_hash: SemanticHash,
    evidence: &PublicationEvidence,
) -> Result<(), CutoverError> {
    if !FIRST_CUTOVER_IDS.contains(&feature.as_str()) {
        return Err(CutoverError::WrongFeatureSet);
    }
    publish_feature(
        PublicationMode::StableLanguage,
        lock,
        feature,
        AuthorityEntry {
            state: AuthorityState::RollbackPending,
            active_source: "legacy".to_string(),
            semantic_hash: legacy_hash.clone(),
        },
        evidence.clone(),
    )
    .map_err(|error| CutoverError::Publication(format!("{error:?}")))?;
    publish_feature(
        PublicationMode::StableLanguage,
        lock,
        feature,
        AuthorityEntry {
            state: AuthorityState::LegacyActive,
            active_source: "legacy".to_string(),
            semantic_hash: legacy_hash,
        },
        evidence.clone(),
    )
    .map_err(|error| CutoverError::Publication(format!("{error:?}")))?;
    Ok(())
}
