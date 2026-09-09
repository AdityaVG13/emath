use std::str::FromStr;
use emath_artifact::{AuthorityEntry, AuthorityLock, AuthorityState};
use emath_build::{PublicationError, PublicationEvidence, PublicationMode, authority_status, publish_feature};
use emath_core::{FeatureId, SemanticHash};
use emath_test_harness::Probe;

fn hash(digit: char) -> SemanticHash {
    SemanticHash::from_str(&format!("sha256:{}", digit.to_string().repeat(64))).unwrap()
}
fn evidence() -> PublicationEvidence {
    PublicationEvidence {
        schemas_valid: true, reference_vectors_valid: true, deterministic_image: true, unrealized_coverage_explicit: true,
        projections_complete: true, live_adapter: true, unique_authority: true, no_blocking_holes: true,
        migrations_valid: true, independent_conformance: true, generated_views_fresh: true, authorized_semantic_change: true,
        conformance: vec!["test://add-exact".into()], generated_views: vec!["doc://reference/add".into()], rollback: "distribution-sha256:prior".into(),
    }
}
fn fixture() -> (FeatureId, AuthorityLock) {
    let feature = FeatureId::from_str("std.capability.math.add").unwrap();
    let mut lock = AuthorityLock::default();
    lock.entries.insert(feature.clone(), AuthorityEntry { state: AuthorityState::LegacyActive, active_source: "legacy".into(), semantic_hash: hash('0') });
    (feature, lock)
}

#[test]
fn probe() {
    let mut p = Probe::new("feature publication scopes candidate/stable/rollback and gates stable on every missing gate");
    p.case("scoped-flow", |p| {
        let (feature, mut lock) = fixture();
        let other = FeatureId::from_str("std.type.int").unwrap();
        lock.entries.insert(other.clone(), AuthorityEntry { state: AuthorityState::LegacyActive, active_source: "legacy".into(), semantic_hash: hash('9') });
        for (mode, state, source) in [
            (PublicationMode::Framework, AuthorityState::CapsuleCandidate, "capsule"),
            (PublicationMode::CandidateImage, AuthorityState::LegacyActiveDualRun, "legacy"),
        ] {
            if publish_feature(mode, &mut lock, &feature, AuthorityEntry { state, active_source: source.into(), semantic_hash: hash('1') }, evidence()).is_err() {
                p.fail(format!("flow/{state:?}"), "must publish");
            }
        }
        match publish_feature(PublicationMode::StableLanguage, &mut lock, &feature, AuthorityEntry { state: AuthorityState::CapsuleActive, active_source: "capsule".into(), semantic_hash: hash('1') }, evidence()) {
            Ok(stable) => p.contains("rollback-hash", &stable.canonical(), "old_hash=sha256:"),
            Err(e) => p.fail("stable", format!("must publish: {e:?}")),
        };
        p.eq("other-hash", lock.entries[&other].semantic_hash.clone(), hash('9'));
        for state in [AuthorityState::RollbackPending, AuthorityState::LegacyActive] {
            let source = if state == AuthorityState::RollbackPending { "legacy" } else { "legacy" };
            let h = if state == AuthorityState::RollbackPending { hash('1') } else { hash('0') };
            if publish_feature(PublicationMode::StableLanguage, &mut lock, &feature, AuthorityEntry { state, active_source: source.into(), semantic_hash: h }, evidence()).is_err() {
                p.fail(format!("rollback/{state:?}"), "must publish");
            }
        }
        p.eq("other-state", lock.entries[&other].state, AuthorityState::LegacyActive);
    });
    p.case("missing-gates", |p| {
        for (gate, idx) in [("projection-closure", 0), ("live-adapter", 1), ("unique-authority", 2), ("blocking-spec-hole", 3), ("migration", 4), ("independent-conformance", 5), ("generated-views", 6), ("authorized-semantic-change", 7)] {
            let (feature, mut lock) = fixture();
            lock.entries.get_mut(&feature).unwrap().state = AuthorityState::LegacyActiveDualRun;
            let mut ev = evidence();
            match idx {
                0 => ev.projections_complete = false,
                1 => ev.live_adapter = false,
                2 => ev.unique_authority = false,
                3 => ev.no_blocking_holes = false,
                4 => ev.migrations_valid = false,
                5 => ev.independent_conformance = false,
                6 => ev.generated_views_fresh = false,
                _ => ev.authorized_semantic_change = false,
            }
            p.eq(gate, publish_feature(PublicationMode::StableLanguage, &mut lock, &feature, AuthorityEntry { state: AuthorityState::CapsuleActive, active_source: "capsule".into(), semantic_hash: hash('1') }, ev), Err(PublicationError::MissingGate(gate)));
        }
        let (feature, mut lock) = fixture();
        p.demand("dual-authority", publish_feature(PublicationMode::Framework, &mut lock, &feature, AuthorityEntry { state: AuthorityState::CapsuleCandidate, active_source: "legacy+capsule".into(), semantic_hash: hash('1') }, evidence()).is_err(), "dual authority must refuse");
        p.eq("status", authority_status(&lock), vec![(feature, AuthorityState::LegacyActive)]);
    });
    p.finish();
}
