//! Version-policy: unparseable versions never satisfy SemverMajor;
//! Exact is identity.

use emath_schema::VersionPolicy;
use emath_test_harness::Probe;

#[test]
fn version_policy() {
    let mut p = Probe::new("unparseable versions never pass SemverMajor; Exact is identity");
    p.demand("nightly-vs-semver", !VersionPolicy::SemverMajor.accepts("nightly", "1.2.3"), "nightly");
    p.demand("semver-vs-local", !VersionPolicy::SemverMajor.accepts("1.2.3", "local"), "local");
    p.demand("nightly-vs-local", !VersionPolicy::SemverMajor.accepts("nightly", "local"), "both garbage");
    p.demand("same-major", VersionPolicy::SemverMajor.accepts("1.9.0", "1.2.3"), "1.x");
    p.demand("next-major", !VersionPolicy::SemverMajor.accepts("2.0.0", "1.2.3"), "2.x must refuse");
    p.demand("exact-eq", VersionPolicy::Exact.accepts("1.2.3", "1.2.3"), "equal");
    p.demand("exact-ne", !VersionPolicy::Exact.accepts("1.2.3", "1.2.4"), "patch");
    p.finish();
}
