//! seam tests migrated from the in-crate `#[cfg(test)]` module.
use emath_adapter_dew::seam::*;
use emath_test_harness::Probe;

#[test]
fn probe() {
    let mut p = Probe::new("dew seam embeds the locked 40-hex upstream commit");
    p.eq("commit-len", AdapterSeam::LOCKED_UPSTREAM_COMMIT.len(), 40);
    p.demand("commit-hex", AdapterSeam::LOCKED_UPSTREAM_COMMIT.chars().all(|c| c.is_ascii_hexdigit()), "must be hex");
    p.contains("seam-pins-commit", &AdapterSeam::current().version.upstream, AdapterSeam::LOCKED_UPSTREAM_COMMIT);
    p.finish();
}
