//! Engine identity tests (origin `crates/emath-lab-core/src/identity.rs`).

use emath_lab_core::EngineIdentity;

#[test]
fn distinct_identities_pass_the_comparator_gate() {
    let subject = EngineIdentity::subject("emath-HEAD-a1401c0");
    let oracle = EngineIdentity::oracle("emath-spec-oracle");
    assert!(subject.require_distinct(&oracle, "evaluate_paired").is_ok());
    assert!(oracle.require_distinct(&subject, "evaluate_paired").is_ok());
}

#[test]
fn identical_identities_are_refused_by_the_comparator_gate() {
    let subject = EngineIdentity::subject("emath-HEAD-a1401c0");
    let clone = EngineIdentity::subject("emath-HEAD-a1401c0");
    let error = subject
        .require_distinct(&clone, "evaluate_paired")
        .unwrap_err();
    assert_eq!(error.code, "E-HOST-016");
    assert!(error.message.contains("evaluate_paired"));
    // Same role, different labels: still a valid comparison.
    let other = EngineIdentity::subject("emath-HEAD-b2e8d00");
    assert!(subject.require_distinct(&other, "evaluate_paired").is_ok());
}

#[test]
fn tokens_are_role_label_stable() {
    assert_eq!(
        EngineIdentity::oracle("emath-spec-oracle").token(),
        "oracle:emath-spec-oracle"
    );
    assert_eq!(
        EngineIdentity::subject("emath-HEAD-a1401c0").token(),
        "subject:emath-HEAD-a1401c0"
    );
}
