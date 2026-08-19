//! Bounded parse forest tests (origin `crates/emath-genesis/src/forest.rs`).

use emath_genesis::forest::{ForestLimits, build_forest, infer_signature};
use emath_term::SymbolId;
use emath_world_ir::Fixity;

/// SGK-G1-003: a trailing operator is hypothesized as postfix
/// application, and the fixity map reports it. `a ⋈` previously had
/// no complete parse at all, so no pinned artifact can change.
#[test]
fn trailing_operator_is_hypothesized_as_postfix() {
    let inference =
        infer_signature("a \u{22c8}", &ForestLimits::default()).expect("postfix parse");
    let symbol = SymbolId("\u{22c8}".to_string());
    assert_eq!(inference.signature.arity(&symbol), Some(1));
    assert_eq!(inference.fixities.get(&symbol), Some(&Fixity::Postfix));
    let term = build_forest("a \u{22c8}", &ForestLimits::default())
        .unique_term()
        .expect("unique postfix term");
    assert_eq!(term.canonical(), "apply(\u{22c8},var(a))");
}

/// Fixity-hypothesis priority is deterministic: a symbol seen both
/// infix and trailing resolves to Infix, never run-dependent.
#[test]
fn fixity_priority_prefers_infix_over_postfix() {
    let inference = infer_signature(
        "(a \u{22c8} b) \u{22c8} (c \u{22c8})",
        &ForestLimits::default(),
    )
    .expect("mixed-position parse");
    let symbol = SymbolId("\u{22c8}".to_string());
    assert_eq!(inference.fixities.get(&symbol), Some(&Fixity::Infix));
}

/// SGK-G1-006/007: the ranking policy and receipts are deterministic —
/// rebuilding the same ambiguous body yields byte-identical canonical
/// JSON, the same parse id, and the same ambiguity count.
#[test]
fn ranking_and_receipts_are_deterministic_across_rebuilds() {
    let body = "\u{29d6}(a \u{22c8} b) \u{229b} \u{03b6}";
    let first = build_forest(body, &ForestLimits::default());
    let second = build_forest(body, &ForestLimits::default());
    assert_eq!(first.canonical_json(), second.canonical_json());
    assert_eq!(first.parse_id(), second.parse_id());
    assert_eq!(first.ambiguity_count(), second.ambiguity_count());
    assert_eq!(first.ambiguity_count(), 1, "reference body stays unique");
}

#[test]
fn application_argument_queue_is_bounded() {
    // Six comma-separated arguments, each with a highly ambiguous
    // split: the extension queue multiplies by `max_alternatives` per
    // comma and must be capped instead of exploding (the 128^n blast).
    let limits = ForestLimits {
        max_nodes: 4096,
        max_alternatives: 16,
        max_depth: 64,
    };
    let started = std::time::Instant::now();
    let forest = build_forest(
        "f(a b c d e, a b c d e, a b c d e, a b c d e, a b c d e, a b c d e)",
        &limits,
    );
    assert!(
        started.elapsed() < std::time::Duration::from_secs(10),
        "application-argument queue must stay bounded"
    );
    assert!(
        forest.node_count() <= limits.max_nodes,
        "node budget must hold: {} > {}",
        forest.node_count(),
        limits.max_nodes
    );
    assert!(
        !forest.holes().is_empty(),
        "ambiguous application must exceed a budget, got {} nodes",
        forest.node_count()
    );
}
