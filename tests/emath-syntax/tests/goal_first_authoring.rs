//! Goal-first authoring: plot, solve, simulate, compile, differentiate, integrate.

use emath_syntax::{expand_scratch, parse_str};

fn has_error(text: &str, code: &str) -> bool {
    let (_, diagnostics) = parse_str(text);
    diagnostics.errors().any(|error| error.code == code)
}

fn parse_ok(p: &mut Probe, text: &str) -> emath_core::tree::SyntaxTree {
    let (tree, diagnostics) = parse_str(text);
    p.demand(
        "parse_ok",
        !diagnostics.has_errors(),
        format!(
            "must parse cleanly, got {:?}",
            diagnostics
                .errors()
                .map(|error| format!("{} {}", error.code, error.message))
                .collect::<Vec<_>>()
        ),
    );
    tree
}

use emath_test_harness::{Probe, boot};

#[test]
fn goal_first_authoring() {
    boot();
    let mut probe = Probe::new("Goal-first authoring: plot, solve, simulate, compile, differentiate, integrate.");
    probe.case("each_intent_verb_expands", |p| {
    let f0 = p.failures().len();

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
        p.demand("1",expansion.rewritten() && expansion.expanded.contains(needle), format!(
            "verb `{source}` must expand containing `{needle}`, got {}",
            expansion.expanded
        ));
        if p.failures().len() != f0 { return; }
        p.demand("2",!expansion.diagnostics.has_errors(), format!(
            "verb `{source}` errors: {:?}",
            expansion
                .diagnostics
                .errors()
                .map(|e| e.code)
                .collect::<Vec<_>>()
        ));
        if p.failures().len() != f0 { return; }
    }

    });
    probe.case("solve_without_domain_labels_candidates", |p| {
    let f0 = p.failures().len();

    let expansion = expand_scratch("solve x^2 = 2\n");
    p.demand("1",expansion.rewritten(), stringify!(expansion.rewritten()));
    if p.failures().len() != f0 { return; }
    p.demand("2",expansion
            .notes
            .iter()
            .any(|note| note.inferred.contains("Complex") && note.inferred.contains("Real")), format!(
        "candidates must be labeled, got {:?}",
        expansion.notes
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("goal_first_example_file_parses", |p| {

    let source = "plot sin(x) on -3.141592653589793..3.141592653589793\nsolve x^2 = 2 over Real\nconvert 1 km to m\ndifferentiate x^2 wrt x\nintegrate x^2 on 0..1\ncompile this to rust.library\nsimulate damped mass spring for 10 s\n";
    let _tree = parse_ok(p, source);

    });
    probe.case("hidden_solve_default_is_e_syn_146", |p| {
    let f0 = p.failures().len();

    let source = include_str!("../../../tests/invalid/goal_first_authoring.emath");
    p.demand("1",has_error(source, "E-SYN-146"), format!(
        "hiding solve candidates must refuse with E-SYN-146"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.finish();
}
