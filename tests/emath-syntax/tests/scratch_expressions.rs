//! L0 scratch grammar: expressions, plot, solve, convert without declarations.

use emath_core::limits::Limits;
use emath_core::tree::{Item, StmtKind};
use emath_exec_ir::interp::Value;
use emath_sema::CompilerSession;
use emath_syntax::{expand_scratch, parse_str};
use std::collections::BTreeMap;

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

    emath_syntax::install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let checked = session.check_owned("l0-two-plus-two", "2+2\n");
    assert!(
        !checked.diagnostics.has_errors(),
        "{:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    );
    let report = emath_exec_ir::runner::run_package(&checked.package);
    assert_eq!(
        report.declarations[0].tests[0].definitions.get("result"),
        Some(&Value::F64(4.0))
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

    emath_syntax::install_source_parser();
    for (name, source, given, expected, tolerance) in [
        (
            "plot",
            "plot sin(x) on -3.14..3.14\n",
            BTreeMap::from([("x".to_string(), Value::F64(0.0))]),
            0.0,
            0.0,
        ),
        (
            "solve",
            "solve x^2 = 2 over Real\n",
            BTreeMap::from([("x".to_string(), Value::F64(1.0))]),
            2.0_f64.sqrt(),
            1e-10,
        ),
        (
            "convert",
            "convert 1 km to m\n",
            BTreeMap::new(),
            1000.0,
            0.0,
        ),
    ] {
        let mut session = CompilerSession::new(Limits::default());
        let checked = session.check_owned(name, source);
        assert!(
            !checked.diagnostics.has_errors(),
            "{name}: {:?}",
            checked.diagnostics.errors().collect::<Vec<_>>()
        );
        let report = emath_exec_ir::runner::run_package_with_given(&checked.package, Some(&given));
        let value = report.declarations[0].tests[0]
            .definitions
            .values()
            .last()
            .expect("intent computes a result");
        let Value::F64(actual) = value else {
            panic!("{name} must compute a scalar, got {value:?}");
        };
        assert!(
            (actual - expected).abs() <= tolerance,
            "{name}: expected {expected}, got {actual}"
        );
    }
}

#[test]
fn mix_scratch_and_declaration_is_e_syn_141() {
    let source = include_str!("../../../tests/invalid/scratch_expressions.emath");
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
