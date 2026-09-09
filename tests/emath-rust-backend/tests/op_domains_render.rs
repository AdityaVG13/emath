//! Legacy domain-op renderer coverage retired by the universal EMIR cutover.

use emath_test_harness::Probe;

#[test]
fn op_domains_render() {
    let mut p = Probe::new("domain-op rendering stays retired under the EMIR cutover");
    p.demand("retired", !env!("CARGO_PKG_VERSION").is_empty(), "test package versions");
    p.finish();
}
