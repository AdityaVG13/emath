//! genesis_cmd tests migrated from the in-crate `#[cfg(test)]` module.
use emath_cli_lab::genesis_cmd::{answer_policy, confined_artifact_id};
use emath_cli_lab::meaning_cmd::ResolvedLock;
use emath_cli::portfolio::{CollapsePolicy, InterpretationPolicy};
use emath_test_harness::{Case, Probe, check_all, expect_ok};
fn lock() -> ResolvedLock { ResolvedLock { lock_id: 1, origin_receipt_id: 2, fingerprint: 9, method: "cli-set".into() } }
#[test]
fn probe() {
    let mut p = Probe::new("genesis answer policy follows lock, artifact ids stay confined");
    p.case("answer-policy", |p| {
        p.demand("portfolio", matches!(answer_policy(true, None), InterpretationPolicy::Portfolio), "flag forces portfolio");
        p.demand("locked", matches!(answer_policy(false, Some(&lock())), InterpretationPolicy::UserLocked { lock_id: 1, .. }), "lock wins");
        p.demand("unique", matches!(answer_policy(false, None), InterpretationPolicy::SingleBest { collapse: CollapsePolicy::RequireUnique }), "default is unique");
    });
    expect_ok(check_all(&[Case::new("hex", "0123456789abcdef", true), Case::new("empty", "", false), Case::new("dotdot", "..", false), Case::new("traversal", "../secret", false), Case::new("slash", "foo/bar", false), Case::new("abs", "/etc/passwd", false), Case::new("nul", "foo\0bar", false)], |s| confined_artifact_id(s)));
    p.demand("traversal-refused", !confined_artifact_id("../x"), "parent traversal never confines");
    p.finish();
}
