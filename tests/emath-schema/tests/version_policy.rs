//! Version-policy witnesses for: unparseable versions must never
//! satisfy a SemverMajor gate.

use emath_schema::VersionPolicy;

#[test]
fn unparseable_versions_never_accept_semver_major() {
    assert!(!VersionPolicy::SemverMajor.accepts("nightly", "1.2.3"));
    assert!(!VersionPolicy::SemverMajor.accepts("1.2.3", "local"));
    assert!(!VersionPolicy::SemverMajor.accepts("nightly", "local"));
}

#[test]
fn semver_major_matches_parsed_majors_only() {
    assert!(VersionPolicy::SemverMajor.accepts("1.9.0", "1.2.3"));
    assert!(!VersionPolicy::SemverMajor.accepts("2.0.0", "1.2.3"));
}

#[test]
fn exact_policy_is_unchanged() {
    assert!(VersionPolicy::Exact.accepts("1.2.3", "1.2.3"));
    assert!(!VersionPolicy::Exact.accepts("1.2.3", "1.2.4"));
}
