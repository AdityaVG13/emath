//! Recipe sections and named kernels are not constructor surface.

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

/// A recipe-shaped file: an `emath function` carrying a `goals:` section.
/// The recipe fixtures that used to live under
/// `tests/fixtures/language/intro/` were pruned with the recipe lane, so
/// the refusal is pinned with an inline source instead.
const RECIPE_SECTIONS: &str = "\
emath function Solve:
    inputs:
        x: Float64
    outputs:
        y: Float64
    definitions:
        y = x
    goals:
        evaluate <y>:
            produce rust.library
";

#[test]
fn doctrine() {
    boot();
    let mut p = Probe::new("recipe sections and named kernels refuse");
    p.case("recipe-sections-gone", |p| {
        Source::from_str("recipe-sections", RECIPE_SECTIONS).must_refuse(&mut *p, &["E-SEC-101"]);
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

