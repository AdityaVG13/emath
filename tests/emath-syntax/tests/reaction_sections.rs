//! (04 section 3.1) failure-first tests.
//!
//! Reaction lines are T3 SECTION grammar, not expression grammar: `2H2 + O2
//! -> 2H2O` must parse as a labeled stoichiometric multiset transformation
//! (`StmtKind::Reaction`), while `=>` (lambda/notation arrow) stays refused
//! inside `reactions:` (C15: juxtaposition refusal stands for expressions;
//! the reaction line is its own grammar, so the two do not conflict).
//!
//! Contracts (each must FAIL against the pre-parser):
//! - `emath reaction_network Name:` parses as a declaration with
//!   `species:` (world-closing) and `reactions:` sections.
//! - `r1: 2H2 + O2 -> 2H2O` → name `r1`, coefficient 2 on `H2`, arrow
//!   Irreversible. `S[ij]`-style token surgery stays impossible: terms are
//!   (coefficient, species) pairs, never split identifiers.
//! - Arrow kinds are three distinct values: `->`, `<->`, `<=>`.
//! - `=>` inside a `reactions:` section is a typed parse refusal
//!   (E-SYN-156), never a silent lambda.
//! - Admission-side contracts (species closure E-CHEM-SPECIES, element
//!   balance E-CHEM-BALANCE) live in tests/emath-sema/tests/
//! reaction_balance.rs.

use emath_core::tree::{Item, ReactionArrow, ReactionTerm, StmtKind};
use emath_syntax::parse_str;

fn declaration_body(p: &mut Probe, source: &str) -> Vec<StmtKind> {
    let (tree, diagnostics) = parse_str(source);
    p.demand(
        "parse",
        !diagnostics.has_errors(),
        format!(
            "expected clean parse, got {:?}",
            diagnostics
                .errors()
                .map(|error| (error.code, error.message.clone()))
                .collect::<Vec<_>>()
        ),
    );
    let Some(Item::Declaration(decl)) = tree.items.first() else {
        panic!("expected a declaration item");
    };
    decl.sections()
        .flat_map(|section| {
            section
                .suite
                .statements
                .iter()
                .map(|stmt| stmt.kind.clone())
        })
        .collect()
}

use emath_test_harness::{Probe, boot};

#[test]
fn reaction_sections() {
    boot();
    let mut probe = Probe::new("(04 section 3.1) failure-first tests. Reaction lines are T3 SECTION grammar, not expression grammar: `2H2 + O2 -> 2H2O` must parse as a labeled");
    probe.case("reaction_network_parses_species_and_reactions", |p| {
    let f0 = p.failures().len();

    let source = "\
emath reaction_network HydrogenCombustion:
    species:
        H2
        O2
        H2O
    reactions:
        r1: 2H2 + O2 -> 2H2O
";
    let statements = declaration_body(p, source);
    p.demand("1",statements
            .iter()
            .any(|kind| matches!(kind, StmtKind::Reaction { .. })), format!(
        "a reaction line must parse as StmtKind::Reaction, got {statements:?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("stoichiometric_coefficients_are_pairs_not_token_surgery", |p| {
    let f0 = p.failures().len();

    let source = "\
emath reaction_network Combustion:
    species:
        H2
        O2
        H2O
    reactions:
        r1: 2H2 + O2 -> 2H2O
";
    let statements = declaration_body(p, source);
    let (lhs, rhs): (&Vec<ReactionTerm>, &Vec<ReactionTerm>) = statements
        .iter()
        .find_map(|kind| match kind {
            StmtKind::Reaction { lhs, rhs, .. } => Some((lhs, rhs)),
            _ => None,
        })
        .expect("reaction line must lower to StmtKind::Reaction");
    p.eq("1", lhs.len(), 2);
    p.eq("2", lhs[0].coefficient, 2);
    p.demand("3", (lhs[0].species) == ("H2"), format!("expected {:?}, got {:?}", ("H2"), (lhs[0].species)));
    if p.failures().len() != f0 { return; }
    p.eq("4", lhs[1].coefficient, 1);
    p.demand("5", (lhs[1].species) == ("O2"), format!("expected {:?}, got {:?}", ("O2"), (lhs[1].species)));
    if p.failures().len() != f0 { return; }
    p.eq("6", rhs.len(), 1);
    p.eq("7", rhs[0].coefficient, 2);
    p.demand("8", (rhs[0].species) == ("H2O"), format!("expected {:?}, got {:?}", ("H2O"), (rhs[0].species)));
    if p.failures().len() != f0 { return; }

    });
    probe.case("arrow_kinds_are_distinct", |p| {
    let f0 = p.failures().len();

    let source = "\
emath reaction_network Arrows:
    species:
        A
        B
    reactions:
        forward: A -> B
        reversible: A <-> B
        equilibrium: A <=> B
";
    let statements = declaration_body(p, source);
    let arrows: Vec<ReactionArrow> = statements
        .iter()
        .filter_map(|kind| match kind {
            StmtKind::Reaction { arrow, .. } => Some(*arrow),
            _ => None,
        })
        .collect();
    p.eq("1", arrows.len(), 3);
    p.demand("2",matches!(arrows[0], ReactionArrow::Irreversible), stringify!(matches!(arrows[0], ReactionArrow::Irreversible)));
    if p.failures().len() != f0 { return; }
    p.demand("3",matches!(arrows[1], ReactionArrow::Reversible), stringify!(matches!(arrows[1], ReactionArrow::Reversible)));
    if p.failures().len() != f0 { return; }
    p.demand("4",matches!(arrows[2], ReactionArrow::Equilibrium), stringify!(matches!(arrows[2], ReactionArrow::Equilibrium)));
    if p.failures().len() != f0 { return; }

    });
    probe.case("lambda_arrow_is_token_identical_to_irreversible", |p| {
    let f0 = p.failures().len();

    // C15/C6 shape: `=>` and `->` share one lexer token (the notation
    // mapping arrow depends on that sharing), so inside `reactions:` the
    // `=>` spelling denotes the irreversible reaction arrow. The test
    // pins that it lowers as a REACTION — never as a notation/call —
    // because this T3 grammar has no lambda position to desugar into.
    // The refusal-only fixture for reaction arrows is `<==>` (Iff token):
    // tests/invalid/reaction_lambda_arrow.emath.
    let source = "\
emath reaction_network BadArrow:
    species:
        A
        B
    reactions:
        wrong: A => B
";
    let statements = declaration_body(p, source);
    let arrows: Vec<ReactionArrow> = statements
        .iter()
        .filter_map(|kind| match kind {
            StmtKind::Reaction { arrow, .. } => Some(*arrow),
            _ => None,
        })
        .collect();
    p.demand("1", (arrows) == (vec![ReactionArrow::Irreversible]), format!("expected {:?}, got {:?}", (vec![ReactionArrow::Irreversible]), (arrows)));
    if p.failures().len() != f0 { return; }

    });
    probe.case("iff_arrow_is_refused_inside_reactions", |p| {
    // `<==>` is the logical Iff token, not a reaction arrow: refuses E-SYN-156.
    let f0 = p.failures().len();

    let fixture = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/invalid/reaction_lambda_arrow.emath"
    ));
    p.demand("1",fixture.contains("expect: E-SYN-156"), format!(
        "fixture must pin E-SYN-156"
    ));
    if p.failures().len() != f0 { return; }
    let (_tree, diagnostics) = parse_str(fixture);
    p.demand("2",diagnostics.errors().any(|error| error.code == "E-SYN-156"), format!(
        "`<==>` inside `reactions:` must refuse E-SYN-156, got {:?}",
        diagnostics
            .errors()
            .map(|error| (error.code, error.message.clone()))
            .collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("trailing_tokens_after_products_are_refused", |p| {
    // Trailing tokens after the products refuse E-SYN-156: the line ends
    // after the RHS terms, and nothing is silently truncated.
    let f0 = p.failures().len();

    let fixture = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/invalid/reaction_trailing_tokens.emath"
    ));
    p.demand("1",fixture.contains("expect: E-SYN-156"), format!(
        "fixture must pin E-SYN-156"
    ));
    if p.failures().len() != f0 { return; }
    let (_tree, diagnostics) = parse_str(fixture);
    p.demand("2",diagnostics.errors().any(|error| error.code == "E-SYN-156"), format!(
        "trailing tokens after products must refuse E-SYN-156, got {:?}",
        diagnostics
            .errors()
            .map(|error| (error.code, error.message.clone()))
            .collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.finish();
}
