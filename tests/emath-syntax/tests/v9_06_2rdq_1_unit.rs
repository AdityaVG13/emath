//! L0 scratch grammar: expressions, plot, solve, convert without declarations.

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

fn first_decl<'a>(tree: &'a emath_core::tree::SyntaxTree) -> &'a emath_core::tree::Declaration {
    let Item::Declaration(decl) = &tree.items[0] else {
        panic!("expected a declaration");
    };
    decl
}

#[test]
fn two_plus_two_parses_as_implicit_function() {
    let tree = parse_ok("2+2\n");
    let decl = first_decl(&tree);
    assert_eq!(decl.name, "Scratch");
    assert!(
        decl.body.iter().any(
            |stmt| matches!(&stmt.kind, StmtKind::Section(section) if section.name == "definitions")
        ),
        "L0 must lower to definitions, got {:?}",
        decl.body
    );
}

#[test]
fn two_plus_two_example_file_parses() {
    let source = include_str!("../../../language/examples/intro/scratch.emath");
    let _tree = parse_ok(source);
}

#[test]
fn plot_solve_convert_expand() {
    let plot = expand_scratch("plot sin(x) on -3.14..3.14\n");
    assert!(plot.rewritten(), "plot must wrap");
    assert!(plot.expanded.contains("sin(x)"), "{}", plot.expanded);
    assert!(
        plot.expanded.contains("emath function Scratch:"),
        "{}",
        plot.expanded
    );

    let solve = expand_scratch("solve x^2 = 2 over Real\n");
    assert!(solve.rewritten());
    assert!(
        solve.expanded.contains("solve(residual) wrt x"),
        "{}",
        solve.expanded
    );
    assert!(
        solve
            .notes
            .iter()
            .any(|note| note.inferred.contains("Real")),
        "{:?}",
        solve.notes
    );

    let convert = expand_scratch("convert 1 km to m\n");
    assert!(convert.rewritten());
    assert!(
        convert.expanded.contains("(1 km) / (1 m)"),
        "{}",
        convert.expanded
    );
}

#[test]
fn mix_scratch_and_declaration_is_e_syn_141() {
    let source = include_str!("../../../tests/invalid/v9_06_2rdq_1.emath");
    assert!(
        has_error(source, "E-SYN-141"),
        "mixed scratch + declaration must refuse with E-SYN-141"
    );
}

#[test]
fn junk_words_are_e_syn_145_not_a_silent_function() {
    assert!(
        has_error("this is not emath at all\n", "E-SYN-145"),
        "non-expression scratch must refuse"
    );
}
