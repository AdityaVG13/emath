//! genesis_cmd tests migrated from the in-crate `#[cfg(test)]` module.

use emath_cli::meaning_cmd::ResolvedLock;
use emath_cli::portfolio::{CollapsePolicy, InterpretationPolicy};
use emath_cli::genesis_cmd::{answer_policy, confined_artifact_id};

fn lock() -> ResolvedLock {
    ResolvedLock {
        lock_id: 1,
        origin_receipt_id: 2,
        fingerprint: 9,
        method: "cli-set".into(),
    }
}

#[test]
fn answer_policy_portfolio_lock_and_unique() {
    assert!(matches!(
        answer_policy(true, None),
        InterpretationPolicy::Portfolio
    ));
    assert!(matches!(
        answer_policy(false, Some(&lock())),
        InterpretationPolicy::UserLocked { lock_id: 1, .. }
    ));
    assert!(matches!(
        answer_policy(false, None),
        InterpretationPolicy::SingleBest {
            collapse: CollapsePolicy::RequireUnique
        }
    ));
}

#[test]
fn confined_artifact_id_rejects_traversal() {
    assert!(confined_artifact_id("0123456789abcdef"));
    assert!(!confined_artifact_id(""));
    assert!(!confined_artifact_id(".."));
    assert!(!confined_artifact_id("../secret"));
    assert!(!confined_artifact_id("../x"));
    assert!(!confined_artifact_id("foo/bar"));
    assert!(!confined_artifact_id("/etc/passwd"));
    assert!(!confined_artifact_id("foo\0bar"));
}
