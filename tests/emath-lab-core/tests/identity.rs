//! Engine identity comparator-gate tests.

use emath_lab_core::EngineIdentity;
use emath_test_harness::Probe;

#[test]
fn engine_identity() {
    let mut p = Probe::new("distinct engines compare, identical engines refuse");
    p.case("distinct-pass", |p| {
        let subject = EngineIdentity::subject("emath-HEAD-a1401c0");
        let oracle = EngineIdentity::oracle("emath-spec-oracle");
        p.demand("subject-oracle", subject.require_distinct(&oracle, "evaluate_paired").is_ok(), "distinct pair compares");
        p.demand("oracle-subject", oracle.require_distinct(&subject, "evaluate_paired").is_ok(), "gate is symmetric");
    });
    p.case("identical-refused", |p| {
        let subject = EngineIdentity::subject("emath-HEAD-a1401c0");
        let error = subject.require_distinct(&EngineIdentity::subject("emath-HEAD-a1401c0"), "evaluate_paired").unwrap_err();
        p.eq("code", error.code, "E-HOST-016");
        p.contains("op", &error.message, "evaluate_paired");
        p.demand("labels-differ-ok", subject.require_distinct(&EngineIdentity::subject("emath-HEAD-b2e8d00"), "evaluate_paired").is_ok(), "same role, different labels compare");
    });
    p.case("tokens", |p| {
        p.eq("oracle", EngineIdentity::oracle("emath-spec-oracle").token(), "oracle:emath-spec-oracle");
        p.eq("subject", EngineIdentity::subject("emath-HEAD-a1401c0").token(), "subject:emath-HEAD-a1401c0");
    });
    p.finish();
}
