//! seam tests migrated from the in-crate `#[cfg(test)]` module.

use emath_adapter_dew::seam::*;

/// The seam exposes the locked upstream commit (40 hex chars), not a
/// bare floating version. `scripts/check_upstream_lock.py` cross-checks
/// this constant against the shipped `forks/UPSTREAM_LOCK.json` row.
#[test]
fn current_seam_embeds_locked_commit() {
    assert_eq!(AdapterSeam::LOCKED_UPSTREAM_COMMIT.len(), 40);
    assert!(
        AdapterSeam::LOCKED_UPSTREAM_COMMIT
            .chars()
            .all(|c| c.is_ascii_hexdigit())
    );
    let seam = AdapterSeam::current();
    assert!(
        seam.version
            .upstream
            .contains(AdapterSeam::LOCKED_UPSTREAM_COMMIT)
    );
}
