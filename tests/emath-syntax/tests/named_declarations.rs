//! L2 named-declaration shorthand (`emath function Name:`).

use emath_core::tree::{Item, StmtKind};
use emath_core::limits::Limits;
use emath_exec_ir::interp::Value;
use emath_sema::CompilerSession;
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
fn named_shorthand_lowers_to_definitions() {
    let source = "emath function Square:\n    y = x^2\n";
    let expansion = expand_scratch(source);
    assert!(
        expansion.rewritten(),
        "L2 must rewrite: {}",
        expansion.expanded
    );
    assert_eq!(expansion.level().as_str(), "L2");
    let again = expand_scratch(&expansion.expanded);
    assert!(
        !again.rewritten(),
        "L2 product must be Canonical: {}",
        again.expanded
    );
    assert_eq!(again.level().as_str(), "canonical");
    assert!(
        expansion.expanded.contains("emath function Square:"),
        "{}",
        expansion.expanded
    );
    assert!(
        expansion.expanded.contains("definitions:"),
        "{}",
        expansion.expanded
    );
    assert!(
        expansion.expanded.contains("y = x^2"),
        "{}",
        expansion.expanded
    );
    let tree = parse_ok(source);
    let Item::Declaration(decl) = &tree.items[0] else {
        panic!("expected a declaration");
    };
    assert_eq!(decl.name, "Square");
    assert!(
        decl.body.iter().any(
            |stmt| matches!(&stmt.kind, StmtKind::Section(section) if section.name == "definitions")
        ),
        "L2 must lower to definitions, got {:?}",
        decl.body
    );
}

#[test]
fn l2_example_file_parses() {
    let source = "emath function Square:\n    y = x^2\n    example x = 3\n";
    let tree = parse_ok(source);
    let Item::Declaration(decl) = &tree.items[0] else {
        panic!("expected a declaration");
    };
    assert_eq!(decl.name, "Square");

    emath_syntax::install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let checked = session.check_owned("l2-square", source);
    assert!(
        !checked.diagnostics.has_errors(),
        "{:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    );
    let report = emath_exec_ir::runner::run_package(&checked.package);
    assert_eq!(
        report.declarations[0].tests[0].outputs.get("y"),
        Some(&Value::F64(9.0))
    );
}

#[test]
fn contracted_l3_is_not_rewritten() {
    let source = include_str!("../../../language/examples/intro/hello-square.emath");
    let expansion = expand_scratch(source);
    assert!(!expansion.rewritten(), "L3 must stay identity");
    assert_eq!(expansion.level().as_str(), "canonical");
    let again = expand_scratch(&expansion.expanded);
    assert_eq!(again.expanded, expansion.expanded);
    assert!(!again.rewritten());
    assert_eq!(again.level().as_str(), "canonical");
}

#[test]
fn bodyless_named_declaration_is_e_syn_143() {
    let source = include_str!("../../../tests/invalid/l2_named_declaration_bodyless.emath");
    assert!(
        has_error(source, "E-SYN-143"),
        "bodyless L2 must refuse with E-SYN-143, not wrap as L0"
    );
    let expansion = expand_scratch(source);
    assert!(
        !expansion.rewritten(),
        "bodyless L2 must not become Scratch"
    );
    assert!(!expansion.expanded.contains("emath function Scratch:"));
}

#[test]
fn conflicting_signature_is_e_syn_149() {
    let source =
        include_str!("../../../tests/invalid/l2_named_declaration_signature_conflict.emath");
    assert!(
        has_error(source, "E-SYN-149"),
        "header `n` vs body `x` must refuse, not coerce"
    );
    let expansion = expand_scratch(source);
    assert!(!expansion.rewritten(), "conflicting L2 must not rewrite");
}

#[test]
fn matching_head_args_still_expand() {
    let source = "emath function Square(x: Float64):\n    y = x^2\n";
    let expansion = expand_scratch(source);
    assert!(
        expansion.rewritten(),
        "matching head-args must still expand: {}",
        expansion.expanded
    );
    assert!(
        expansion.expanded.contains("definitions:"),
        "{}",
        expansion.expanded
    );
    assert!(
        !has_error(source, "E-SYN-149"),
        "matching names must not look like a signature conflict"
    );
}

#[test]
fn cannot_infer_domain_without_hole_is_e_syn_150() {
    let source =
        include_str!("../../../tests/invalid/l2_named_declaration_cannot_infer_domain.emath");
    assert!(
        has_error(source, "E-SYN-150"),
        "unknown callee `mystery` must refuse, not become a silent input"
    );
    let expansion = expand_scratch(source);
    assert!(!expansion.rewritten(), "unknown-callee L2 must not rewrite");
}

#[test]
fn hole_for_unknown_callee_is_admitted() {
    let source = "emath function Mystery:\n    mystery = ?\n    y = mystery(x)\n";
    let expansion = expand_scratch(source);
    assert!(
        expansion.rewritten(),
        "a hole for `mystery` must admit the L2 body: {}",
        expansion.expanded
    );
    assert!(
        !has_error(source, "E-SYN-150"),
        "declared hole is the domain, not a silent inference"
    );
}
