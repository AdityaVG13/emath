//! Data-driven `emath family` expansion into ordinary capability cells.

use emath_ir::ExprNode;
use emath_syntax::{parse_str};

fn check(name: &str, source: &str) -> emath_sema::admit::CheckResult {
    Source::from_str(name, source).check()
}

use emath_test_harness::{Probe, Source, boot};

#[test]
fn capability_families() {
    boot();
    let mut probe = Probe::new("Data-driven `emath family` expansion into ordinary capability cells.");
    probe.case("elementwise_family_generates_capability_cells", |p| {
    let f0 = p.failures().len();

    let source = include_str!("../../../tests/fixtures/language/intro/capability-families.emath");
    let (_, parse_diagnostics) = parse_str(source);
    p.demand("1",!parse_diagnostics.has_errors(), stringify!(!parse_diagnostics.has_errors()));
    if p.failures().len() != f0 { return; }

    let checked = check("elementwise-family", source);
    p.demand("2",!checked.diagnostics.has_errors(), format!(
        "{:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    p.demand("3", (checked
            .package
            .declarations
            .iter()
            .map(|declaration| declaration.name.leaf())
            .collect::<Vec<_>>()) == (["Exp", "Sin", "Sqrt"]), format!("expected {:?}, got {:?}", (["Exp", "Sin", "Sqrt"]), (checked
            .package
            .declarations
            .iter()
            .map(|declaration| declaration.name.leaf())
            .collect::<Vec<_>>())));
    if p.failures().len() != f0 { return; }
    for (index, declaration) in checked.package.declarations.iter().enumerate() {
        p.eq("4", declaration.id.0 as usize, index);
        p.demand("5", (declaration.kind_label) == ("capability"), format!("expected {:?}, got {:?}", ("capability"), (declaration.kind_label)));
        if p.failures().len() != f0 { return; }
        p.eq("6", declaration.inputs.len(), 1);
        p.eq("7", declaration.outputs.len(), 1);
        let expression = checked
            .package
            .expr(*declaration.definitions.get("value").unwrap())
            .unwrap();
        let ExprNode::Call {
            function,
            arguments,
        } = expression
        else {
            panic!("generated family cell must project through an ordinary call expression");
        };
        p.eq("8", arguments.len(), 1);
        p.eq("9", function.leaf().to_ascii_lowercase(), declaration.name.leaf().to_ascii_lowercase());
    }

    let repeated = check("elementwise-family-repeat", source);
    p.eq("10", checked.package.meaning_id(&[]).unwrap(), repeated.package.meaning_id(&[]).unwrap());

    });
    probe.case("unknown_parameters_and_missing_kind_refuse", |p| {
    let f0 = p.failures().len();

    let invalid = check(
        "invalid-family",
        include_str!("../../../tests/invalid/capability_families.emath"),
    );
    p.demand("1",invalid
            .diagnostics
            .errors()
            .any(|error| error.code == "E-KIND-026"), stringify!(invalid
            .diagnostics
            .errors()
            .any(|error| error.code == "E-KIND-026")));
    if p.failures().len() != f0 { return; }
    p.demand("2",invalid.package.declarations.is_empty(), stringify!(invalid.package.declarations.is_empty()));
    if p.failures().len() != f0 { return; }

    let incomplete = check(
        "incomplete-family",
        "\
use std.kinds.family
emath family ElementwiseUnary<Op>:
    inputs:
        x: Float64
    definitions:
        value = x
    instances:
        \"sin\"
        \"exp\"
        \"sqrt\"
",
    );
    p.demand("3",incomplete
            .diagnostics
            .errors()
            .any(|error| error.code == "E-KIND-003"), stringify!(incomplete
            .diagnostics
            .errors()
            .any(|error| error.code == "E-KIND-003")));
    if p.failures().len() != f0 { return; }
    p.demand("4",incomplete.package.declarations.is_empty(), stringify!(incomplete.package.declarations.is_empty()));
    if p.failures().len() != f0 { return; }

    let missing_kind = check(
        "family-without-import",
        "\
emath family ElementwiseUnary<Op>:
    inputs:
        x: Float64
    outputs:
        value: Float64
    definitions:
        value = x
    instances:
        \"sin\"
        \"exp\"
        \"sqrt\"
",
    );
    p.demand("5",missing_kind
            .diagnostics
            .errors()
            .any(|error| error.code == "E-KIND-100"), stringify!(missing_kind
            .diagnostics
            .errors()
            .any(|error| error.code == "E-KIND-100")));
    if p.failures().len() != f0 { return; }
    p.demand("6",missing_kind.package.declarations.is_empty(), stringify!(missing_kind.package.declarations.is_empty()));
    if p.failures().len() != f0 { return; }

    });
    probe.finish();
}
