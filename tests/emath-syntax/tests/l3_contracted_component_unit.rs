//! L3 contracted-component surface: constructor sections, leftover goals refuse.

use emath_core::tree::Item;
use emath_syntax::parse_str;
use emath_test_harness::{boot, Probe, Source};

fn count_statements(p: &mut Probe, source: &str, section: &str) -> Option<usize> {
    let (tree, diags) = parse_str(source);
    p.demand(
        "parse",
        diags.items().is_empty(),
        format!("constructor L3 source must parse cleanly, got {diags:?}"),
    );
    tree.items.iter().find_map(|item| match item {
        Item::Declaration(declaration) => declaration
            .sections()
            .find(|s| s.name == section)
            .map(|s| s.suite.statements.len()),
        _ => None,
    })
}

#[test]
fn l3_contracted_component_unit() {
    boot();
    let mut probe = Probe::new("constructor L3 sections parse; leftover goals refuse");
    probe.case("l3_contracted_component_parses", |p| {
        let source = "\
emath function Area:
    inputs:
        side: Float64

    outputs:
        area: Float64

    definitions:
        area = side * side
";
        let inputs = count_statements(p, source, "inputs");
        p.eq("1", inputs, Some(1));
        let outputs = count_statements(p, source, "outputs");
        p.eq("2", outputs, Some(1));
        let definitions = count_statements(p, source, "definitions");
        p.eq("3", definitions, Some(1));
    });
    probe.case("goals-gone", |p| {
        Source::from_str(
            "goals-gone",
            "emath function Square:\n    inputs:\n        x: Float64\n    outputs:\n        y: Float64\n    definitions:\n        y = x * x\n    goals:\n        evaluate <y>:\n            produce rust.library\n",
        )
        .must_refuse(p, &["E-SEC-101"]);
    });
    probe.finish();
}
