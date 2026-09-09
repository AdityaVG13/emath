//! Bounded parse forest tests.

use emath_genesis::forest::{ForestLimits, build_forest, infer_signature};
use emath_term::SymbolId;
use emath_world_ir::Fixity;
use emath_test_harness::Probe;

#[test]
fn parse_forest() {
    let mut p = Probe::new("the bounded forest hypothesizes fixity and stays budgeted");
    p.case("trailing-operator-postfix", |p| {
        let inference = infer_signature("a \u{22c8}", &ForestLimits::default()).expect("postfix parse");
        let symbol = SymbolId("\u{22c8}".to_string());
        p.eq("arity", inference.signature.arity(&symbol), Some(1));
        p.eq("fixity", inference.fixities.get(&symbol), Some(&Fixity::Postfix));
        let term = build_forest("a \u{22c8}", &ForestLimits::default()).unique_term().expect("unique postfix term");
        p.eq("term", term.canonical(), "apply(\u{22c8},var(a))".to_string());
    });
    p.case("infix-beats-postfix", |p| {
        let inference = infer_signature("(a \u{22c8} b) \u{22c8} (c \u{22c8})", &ForestLimits::default()).expect("mixed-position parse");
        p.eq("fixity", inference.fixities.get(&SymbolId("\u{22c8}".to_string())), Some(&Fixity::Infix));
    });
    p.case("receipts-deterministic", |p| {
        let body = "\u{29d6}(a \u{22c8} b) \u{229b} \u{03b6}";
        let first = build_forest(body, &ForestLimits::default());
        let second = build_forest(body, &ForestLimits::default());
        p.eq("json", first.canonical_json(), second.canonical_json());
        p.eq("id", first.parse_id(), second.parse_id());
        p.eq("ambiguity", first.ambiguity_count(), second.ambiguity_count());
        p.eq("unique", first.ambiguity_count(), 1);
    });
    p.case("arguments-bounded", |p| {
        let limits = ForestLimits { max_nodes: 4096, max_alternatives: 16, max_depth: 64 };
        let started = std::time::Instant::now();
        let forest = build_forest("f(a b c d e, a b c d e, a b c d e, a b c d e, a b c d e, a b c d e)", &limits);
        p.demand("fast", started.elapsed() < std::time::Duration::from_secs(10), "argument queue stays bounded");
        p.demand("budget", forest.node_count() <= limits.max_nodes, "node budget holds");
        p.demand("holes", !forest.holes().is_empty(), "ambiguous application exceeds a budget");
    });
    p.finish();
}
