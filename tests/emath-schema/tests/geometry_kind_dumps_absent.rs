use emath_test_harness::{Probe, workspace_path};

#[test]
fn geometry_kind_dumps_do_not_ship_under_language_spec() {
    let mut p = Probe::new("geometry kind catalog dumps stay out of language/spec");
    let relative = "language/spec/kinds/geometry.emath";
    p.demand(relative, !workspace_path(relative).exists(), "must not ship");
    p.finish();
}
