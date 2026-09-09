//! Tests for graph.rs, migrated out of production code.
use emath_cli_lab::layout::{LAYOUT_VERSION, LayoutError, check_version, parse_latex};
use emath_test_harness::{Case, Probe, check_all, expect_ok};
#[test]
fn probe() {
    let mut p = Probe::new("layout version gate refuses unknown, latex graph is deterministic");
    expect_ok(check_all(&[Case::new("current", LAYOUT_VERSION, Ok(())), Case::new("next", LAYOUT_VERSION + 1, Err(LayoutError::UnknownVersion { version: LAYOUT_VERSION + 1 }))], |v| check_version(*v)));
    let (a, b) = (parse_latex(r"\sum_{i=1}^{3} i").expect("parse"), parse_latex(r"\sum_{i=1}^{3} i").expect("parse"));
    p.case("canonical", |p| { p.eq("canonical", a.canonical(), b.canonical()); p.eq("graph-id", a.graph_id(), b.graph_id()); p.eq("source", a.source(), r"\sum_{i=1}^{3} i"); });
    p.finish();
}
