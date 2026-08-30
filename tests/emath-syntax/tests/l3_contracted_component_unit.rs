//! L3 contracted-component surface (bead emath-l3-contracted-component-ceus7).
//!
//! Failure-first pins for the canonical L3 declaration shape:
//! `emath <kind> Name:` + inputs/outputs/definitions/goals section blocks,
//! plus the optional `examples:` section surviving the parser.

use emath_core::tree::Item;
use emath_syntax::parse_str;

fn count_statements(source: &str, section: &str) -> Option<usize> {
    let (tree, diags) = parse_str(source);
    assert!(
        diags.items().is_empty(),
        "minimal L3 source must parse cleanly, got {diags:?}"
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
fn l3_contracted_component_parses() {
    let source = "\
emath function Area:
    inputs:
        side: Float64

    outputs:
        area: Float64

    definitions:
        area = side * side

    goals:
        evaluate <area>:
            produce rust.library
";
    assert_eq!(count_statements(source, "inputs"), Some(1));
    assert_eq!(count_statements(source, "outputs"), Some(1));
    assert_eq!(count_statements(source, "definitions"), Some(1));
    assert_eq!(count_statements(source, "goals"), Some(1));
}

#[test]
fn l3_optional_sections_parse() {
    let source = "\
emath function Area:
    inputs:
        side: Float64

    outputs:
        area: Float64

    definitions:
        area = side * side

    examples:
        area = 9.0
";
    assert_eq!(
        count_statements(source, "examples"),
        Some(1),
        "optional `examples:` section must survive parsing"
    );
}
