//! Data-driven `emath family` expansion into ordinary capability cells.

use emath_core::limits::Limits;
use emath_ir::ExprNode;
use emath_sema::CompilerSession;
use emath_syntax::{install_source_parser, parse_str};

fn check(name: &str, source: &str) -> emath_sema::admit::CheckResult {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    session.check_owned(name, source)
}

#[test]
fn v9_06_2rdq_12_elementwise_family_generates_capability_cells() {
    let source = include_str!("../../../language/examples/intro/v9_06_2rdq_12.emath");
    let (_, parse_diagnostics) = parse_str(source);
    assert!(!parse_diagnostics.has_errors());

    let checked = check("elementwise-family", source);
    assert!(
        !checked.diagnostics.has_errors(),
        "{:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    );
    assert_eq!(
        checked
            .package
            .declarations
            .iter()
            .map(|declaration| declaration.name.leaf())
            .collect::<Vec<_>>(),
        ["Exp", "Sin", "Sqrt"]
    );
    for (index, declaration) in checked.package.declarations.iter().enumerate() {
        assert_eq!(declaration.id.0 as usize, index);
        assert_eq!(declaration.kind_label, "capability");
        assert_eq!(declaration.inputs.len(), 1);
        assert_eq!(declaration.outputs.len(), 1);
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
        assert_eq!(arguments.len(), 1);
        assert_eq!(
            function.leaf(),
            declaration.name.leaf().to_ascii_lowercase()
        );
    }

    let repeated = check("elementwise-family-repeat", source);
    assert_eq!(
        checked.package.meaning_id(&[]).unwrap(),
        repeated.package.meaning_id(&[]).unwrap()
    );
}

#[test]
fn v9_06_2rdq_12_unknown_parameters_and_missing_kind_refuse() {
    let invalid = check(
        "invalid-family",
        include_str!("../../../tests/invalid/v9_06_2rdq_12.emath"),
    );
    assert!(
        invalid
            .diagnostics
            .errors()
            .any(|error| error.code == "E-KIND-026")
    );
    assert!(invalid.package.declarations.is_empty());

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
    assert!(
        incomplete
            .diagnostics
            .errors()
            .any(|error| error.code == "E-KIND-003")
    );
    assert!(incomplete.package.declarations.is_empty());

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
    assert!(
        missing_kind
            .diagnostics
            .errors()
            .any(|error| error.code == "E-KIND-100")
    );
    assert!(missing_kind.package.declarations.is_empty());
}
