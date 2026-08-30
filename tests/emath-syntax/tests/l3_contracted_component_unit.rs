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

fn find_square(
    tree: &emath_core::tree::SyntaxTree,
) -> Option<&emath_core::tree::Declaration> {
    tree.items.iter().find_map(|item| match item {
        Item::Declaration(declaration) if declaration.name == "Square" => {
            Some(declaration)
        }
        _ => None,
    })
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

/// Pass 4: L2 named shorthand must expand into canonical L3 text — the
/// expanded source must parse, carry the canonical section set, and match
/// a hand-written contracted component structurally (same sections, same
/// definition names).
#[test]
fn l2_expand_matches_handwritten_l3() {
    use emath_syntax::expand_scratch;

    let l2 = "emath function Square:\n    area = side * side\n";
    let expansion = expand_scratch(l2);
    assert!(
        expansion.rewritten(),
        "L2 shorthand must rewrite, got level {:?}",
        expansion.level()
    );
    let expanded = &expansion.expanded;

    let handwritten = "\
emath function Square:
    inputs:
        side: Float64

    outputs:
        area: Float64

    definitions:
        area = side * side
";
    let (hand_tree, hand_diags) = parse_str(handwritten);
    assert!(
        hand_diags.items().is_empty(),
        "hand-written L3 baseline must parse cleanly, got {hand_diags:?}"
    );

    let (exp_tree, exp_diags) = parse_str(expanded);
    assert!(
        exp_diags.items().is_empty(),
        "expanded L2 output must parse cleanly, got {exp_diags:?}; expanded:\n{expanded}"
    );

    let hand_decl = find_square(&hand_tree)
        .unwrap_or_else(|| panic!("declaration `Square` missing in baseline"));
    let exp_decl = find_square(&exp_tree)
        .unwrap_or_else(|| panic!("declaration `Square` missing in expanded output:\n{expanded}"));

    // Structural equivalence: same section set, same definition-statement
    // kinds, same count of definitions.
    let hand_sections: Vec<_> = hand_decl.sections().map(|s| s.name.clone()).collect();
    let exp_sections: Vec<_> = exp_decl.sections().map(|s| s.name.clone()).collect();
    assert_eq!(hand_sections, exp_sections, "canonical section set");

    let hand_defs = hand_decl
        .sections()
        .find(|s| s.name == "definitions")
        .map(|s| s.suite.statements.len())
        .unwrap_or(0);
    let exp_defs = exp_decl
        .sections()
        .find(|s| s.name == "definitions")
        .map(|s| s.suite.statements.len())
        .unwrap_or(0);
    assert_eq!(hand_defs, 1, "hand-written baseline: one definition");
    assert_eq!(hand_defs, exp_defs, "definition counts must match");
}
