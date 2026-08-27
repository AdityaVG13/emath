//! Intent-verb grammar lowering to goals (find, show, prove, compare, share, build).

use emath_syntax::{expand_scratch, parse_str};

fn has_error(text: &str, code: &str) -> bool {
    let (_, diagnostics) = parse_str(text);
    diagnostics.errors().any(|error| error.code == code)
}

#[test]
fn extra_intent_verbs_expand() {
    for verb in [
        "find f\n",
        "show y\n",
        "prove y = x^2\n",
        "compare Newton and Bisection\n",
        "share this\n",
        "build this\n",
    ] {
        let expansion = expand_scratch(verb);
        assert!(
            expansion.rewritten && !expansion.diagnostics.has_errors(),
            "`{verb}` must expand, got {} {:?}",
            expansion.expanded,
            expansion
                .diagnostics
                .errors()
                .map(|e| e.code)
                .collect::<Vec<_>>()
        );
        assert!(
            expansion.expanded.contains("intent=")
                || expansion.notes.iter().any(|n| n.inferred.contains("goal")),
            "{}",
            expansion.expanded
        );
    }
    let source =
        "find f\nshow y\nprove y = x^2\ncompare Newton and Bisection\nshare this\nbuild this\n";
    let (_, diagnostics) = parse_str(source);
    assert!(
        !diagnostics.has_errors(),
        "{:?}",
        diagnostics.errors().map(|e| e.code).collect::<Vec<_>>()
    );
}

#[test]
fn unknown_verb_is_e_syn_148() {
    let source = include_str!("../../../tests/invalid/v9_06_2rdq_8.emath");
    assert!(has_error(source, "E-SYN-148"));
}
