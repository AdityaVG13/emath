use emath_test_harness::{Probe, workspace_path};

#[test]
fn workflow_kind_dumps_do_not_ship_under_language_spec() {
    let mut p = Probe::new("workflow kind catalog dumps stay out of language/spec");
    let relative = "language/spec/kinds/workflow.emath";
    p.demand(relative, !workspace_path(relative).exists(), "must not ship");
    p.finish();
}
