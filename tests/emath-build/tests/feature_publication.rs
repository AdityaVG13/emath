use std::str::FromStr;

use emath_artifact::{AuthorityEntry, AuthorityLock, AuthorityState};
use emath_build::{
    PublicationError, PublicationEvidence, PublicationMode, authority_status, publish_feature,
};
use emath_core::{FeatureId, SemanticHash};

fn hash(digit: char) -> SemanticHash {
    SemanticHash::from_str(&format!("sha256:{}", digit.to_string().repeat(64))).unwrap()
}

fn evidence() -> PublicationEvidence {
    PublicationEvidence {
        schemas_valid: true,
        reference_vectors_valid: true,
        deterministic_image: true,
        unrealized_coverage_explicit: true,
        projections_complete: true,
        live_adapter: true,
        unique_authority: true,
        no_blocking_holes: true,
        migrations_valid: true,
        independent_conformance: true,
        generated_views_fresh: true,
        authorized_semantic_change: true,
        conformance: vec!["test://add-exact".to_string()],
        generated_views: vec!["doc://reference/add".to_string()],
        rollback: "distribution-sha256:prior".to_string(),
    }
}

fn fixture() -> (FeatureId, AuthorityLock) {
    let feature = FeatureId::from_str("std.capability.math.add").unwrap();
    let mut lock = AuthorityLock::default();
    lock.entries.insert(
        feature.clone(),
        AuthorityEntry {
            state: AuthorityState::LegacyActive,
            active_source: "legacy".to_string(),
            semantic_hash: hash('0'),
        },
    );
    (feature, lock)
}

#[test]
fn framework_candidate_stable_and_rollback_are_feature_scoped() {
    let (feature, mut lock) = fixture();
    let other = FeatureId::from_str("std.type.int").unwrap();
    lock.entries.insert(
        other.clone(),
        AuthorityEntry {
            state: AuthorityState::LegacyActive,
            active_source: "legacy".to_string(),
            semantic_hash: hash('9'),
        },
    );
    let candidate = AuthorityEntry {
        state: AuthorityState::CapsuleCandidate,
        active_source: "capsule".to_string(),
        semantic_hash: hash('1'),
    };
    publish_feature(
        PublicationMode::Framework,
        &mut lock,
        &feature,
        candidate,
        evidence(),
    )
    .unwrap();
    publish_feature(
        PublicationMode::CandidateImage,
        &mut lock,
        &feature,
        AuthorityEntry {
            state: AuthorityState::LegacyActiveDualRun,
            active_source: "legacy".to_string(),
            semantic_hash: hash('1'),
        },
        evidence(),
    )
    .unwrap();
    let stable = publish_feature(
        PublicationMode::StableLanguage,
        &mut lock,
        &feature,
        AuthorityEntry {
            state: AuthorityState::CapsuleActive,
            active_source: "capsule".to_string(),
            semantic_hash: hash('1'),
        },
        evidence(),
    )
    .unwrap();
    assert!(stable.canonical().contains("old_hash=sha256:"));
    assert_eq!(lock.entries[&other].semantic_hash, hash('9'));
    publish_feature(
        PublicationMode::StableLanguage,
        &mut lock,
        &feature,
        AuthorityEntry {
            state: AuthorityState::RollbackPending,
            active_source: "legacy".to_string(),
            semantic_hash: hash('1'),
        },
        evidence(),
    )
    .unwrap();
    publish_feature(
        PublicationMode::StableLanguage,
        &mut lock,
        &feature,
        AuthorityEntry {
            state: AuthorityState::LegacyActive,
            active_source: "legacy".to_string(),
            semantic_hash: hash('0'),
        },
        evidence(),
    )
    .unwrap();
    assert_eq!(lock.entries[&other].state, AuthorityState::LegacyActive);
}

#[test]
fn stable_publication_refuses_every_missing_gate_and_dual_authority() {
    for (gate, mutate) in [
        ("projection-closure", 0usize),
        ("live-adapter", 1),
        ("unique-authority", 2),
        ("blocking-spec-hole", 3),
        ("migration", 4),
        ("independent-conformance", 5),
        ("generated-views", 6),
        ("authorized-semantic-change", 7),
    ] {
        let (feature, mut lock) = fixture();
        lock.entries.get_mut(&feature).unwrap().state = AuthorityState::LegacyActiveDualRun;
        let mut evidence = evidence();
        match mutate {
            0 => evidence.projections_complete = false,
            1 => evidence.live_adapter = false,
            2 => evidence.unique_authority = false,
            3 => evidence.no_blocking_holes = false,
            4 => evidence.migrations_valid = false,
            5 => evidence.independent_conformance = false,
            6 => evidence.generated_views_fresh = false,
            _ => evidence.authorized_semantic_change = false,
        }
        assert_eq!(
            publish_feature(
                PublicationMode::StableLanguage,
                &mut lock,
                &feature,
                AuthorityEntry {
                    state: AuthorityState::CapsuleActive,
                    active_source: "capsule".to_string(),
                    semantic_hash: hash('1')
                },
                evidence
            ),
            Err(PublicationError::MissingGate(gate))
        );
    }
    let (feature, mut lock) = fixture();
    assert!(
        publish_feature(
            PublicationMode::Framework,
            &mut lock,
            &feature,
            AuthorityEntry {
                state: AuthorityState::CapsuleCandidate,
                active_source: "legacy+capsule".to_string(),
                semantic_hash: hash('1')
            },
            evidence()
        )
        .is_err()
    );
    assert_eq!(
        authority_status(&lock),
        vec![(feature, AuthorityState::LegacyActive)]
    );
}
