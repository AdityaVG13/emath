//! L1 guided relationships and examples (`y = x^2 + 4`, `example x = 3`).

use emath_core::tree::{Item, StmtKind};
use emath_syntax::{expand_scratch, parse_str};

fn has_error(text: &str, code: &str) -> bool {
    let (_, diagnostics) = parse_str(text);
    diagnostics.errors().any(|error| error.code == code)
}

fn parse_ok(text: &str) -> emath_core::tree::SyntaxTree {
    let (tree, diagnostics) = parse_str(text);
    assert!(
        !diagnostics.has_errors(),
        "must parse cleanly, got {:?}",
        diagnostics
            .errors()
            .map(|error| format!("{} {}", error.code, error.message))
            .collect::<Vec<_>>()
    );
    tree
}

#[test]
fn relationship_plus_example_infers_input_and_tests() {
    let source = "y = x^2 + 4\nexample x = 3\n";
    let expansion = expand_scratch(source);
    assert!(expansion.rewritten);
    assert_eq!(expansion.level.as_str(), "L1");
    assert!(
        expansion.expanded.contains("inputs:"),
        "{}",
        expansion.expanded
    );
    assert!(
        expansion.expanded.contains("given x = 3"),
        "{}",
        expansion.expanded
    );
    let tree = parse_ok(source);
    let Item::Declaration(decl) = &tree.items[0] else {
        panic!("expected a declaration");
    };
    assert!(
        decl.body.iter().any(
            |stmt| matches!(&stmt.kind, StmtKind::Section(section) if section.name == "tests")
        ),
        "L1 example must lower to tests:, got {:?}",
        decl.body
    );
}

#[test]
fn l1_example_file_parses() {
    let source = include_str!("../../../language/examples/intro/scratch.emath");
    let _tree = parse_ok(source);
}

#[test]
fn conflicting_example_types_are_e_syn_142() {
    let source = include_str!("../../../tests/invalid/v9_06_2rdq_2.emath");
    assert!(
        has_error(source, "E-SYN-142"),
        "conflicting example types must refuse with E-SYN-142"
    );
}
