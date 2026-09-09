//! Intent-verb grammar lowering to goals (find, show, prove, compare, share, build).

use emath_syntax::{expand_scratch, parse_str};

fn has_error(text: &str, code: &str) -> bool {
    let (_, diagnostics) = parse_str(text);
    diagnostics.errors().any(|error| error.code == code)
}

use emath_test_harness::{Probe, boot};

#[test]
fn intent_verbs() {
    boot();
    let mut probe = Probe::new("Intent-verb grammar lowering to goals (find, show, prove, compare, share, build).");
    probe.case("extra_intent_verbs_expand", |p| {
    let f0 = p.failures().len();

    for verb in [
        "find f\n",
        "show y\n",
        "prove y = x^2\n",
        "compare Newton and Bisection\n",
        "share this\n",
        "build this\n",
    ] {
        let expansion = expand_scratch(verb);
        p.demand("1",expansion.rewritten() && !expansion.diagnostics.has_errors(), format!(
            "`{verb}` must expand, got {} {:?}",
            expansion.expanded,
            expansion
                .diagnostics
                .errors()
                .map(|e| e.code)
                .collect::<Vec<_>>()
        ));
        if p.failures().len() != f0 { return; }
        p.demand("2",expansion.expanded.contains("intent=")
                || expansion.notes.iter().any(|n| n.inferred.contains("goal")), format!(
            "{}",
            expansion.expanded
        ));
        if p.failures().len() != f0 { return; }
    }
    let source =
        "find f\nshow y\nprove y = x^2\ncompare Newton and Bisection\nshare this\nbuild this\n";
    let (_, diagnostics) = parse_str(source);
    p.demand("3",!diagnostics.has_errors(), format!(
        "{:?}",
        diagnostics.errors().map(|e| e.code).collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("unknown_verb_is_e_syn_148", |p| {
    let f0 = p.failures().len();

    let source = include_str!("../../../tests/invalid/intent_verbs.emath");
    p.demand("1",has_error(source, "E-SYN-148"), stringify!(has_error(source, "E-SYN-148")));
    if p.failures().len() != f0 { return; }

    });
    probe.finish();
}
