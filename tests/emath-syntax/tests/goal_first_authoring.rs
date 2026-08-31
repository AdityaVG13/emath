//! Goal-first authoring: plot, solve, simulate, compile, differentiate, integrate.

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
fn each_intent_verb_expands() {
    let cases = [
        ("plot sin(x) on -3.14..3.14\n", "sin(x)"),
        ("solve x^2 = 2 over Real\n", "solve(residual) wrt x"),
        ("simulate damped mass spring for 10 s\n", "intent=simulate"),
        ("compile this to rust.library\n", "target rust"),
        ("differentiate x^2 wrt x\n", "derivative(x^2) wrt x"),
        ("integrate x^2 on 0..1\n", "integral"),
        ("convert 1 km to m\n", "(1 km) / (1 m)"),
    ];
    for (source, needle) in cases {
        let expansion = expand_scratch(source);
        assert!(
            expansion.rewritten() && expansion.expanded.contains(needle),
            "verb `{source}` must expand containing `{needle}`, got {}",
            expansion.expanded
        );
        assert!(
            !expansion.diagnostics.has_errors(),
            "verb `{source}` errors: {:?}",
            expansion
                .diagnostics
                .errors()
                .map(|e| e.code)
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn solve_without_domain_labels_candidates() {
    let expansion = expand_scratch("solve x^2 = 2\n");
    assert!(expansion.rewritten());
    assert!(
        expansion
            .notes
            .iter()
            .any(|note| note.inferred.contains("Complex") && note.inferred.contains("Real")),
        "candidates must be labeled, got {:?}",
        expansion.notes
    );
}

#[test]
fn goal_first_example_file_parses() {
    let source = "plot sin(x) on -3.141592653589793..3.141592653589793\nsolve x^2 = 2 over Real\nconvert 1 km to m\ndifferentiate x^2 wrt x\nintegrate x^2 on 0..1\ncompile this to rust.library\nsimulate damped mass spring for 10 s\n";
    let _tree = parse_ok(source);
}

#[test]
fn hidden_solve_default_is_e_syn_146() {
    let source = include_str!("../../../tests/invalid/goal_first_authoring.emath");
    assert!(
        has_error(source, "E-SYN-146"),
        "hiding solve candidates must refuse with E-SYN-146"
    );
}
