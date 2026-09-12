//! Recipe examples and named kernels are not constructor surface.

use emath_test_harness::{Probe, Source, boot};

const GRAPH: &str = "\
emath function negative_edge_shortest_paths:
    inputs:
        source: Float64
    outputs:
        distances: Float64
    definitions:
        distances = bellman_ford(source)
";

#[test]
fn doctrine() {
    boot();
    let mut p = Probe::new("named recipes and unit-catalog imports refuse");
    p.case("recipe-files-gone", |p| {
        for path in [
            "tests/fixtures/language/intro/solve.emath",
            "tests/fixtures/language/intro/jacobian.emath",
        ] {
            Source::from_workspace(path).must_refuse(&mut *p, &["E-SEC-101"]);
        }
    });
    p.case("named-kernel-gone", |p| {
        Source::from_str("graph", GRAPH).must_refuse(&mut *p, &["E-TYPE-002"]);
    });
    p.case("kinds-gone", |p| {
        Source::from_str("kinds", "emath widget Cool:\n    inputs:\n        x: Float64\n")
            .must_refuse(&mut *p, &["E-KIND-GONE"]);
    });
    p.finish();
}
