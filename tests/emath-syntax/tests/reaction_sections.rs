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

fn declaration_body(source: &str) -> Vec<StmtKind> {
    let (tree, diagnostics) = parse_str(source);
    assert!(
        !diagnostics.has_errors(),
        "expected clean parse, got {:?}",
        diagnostics
            .errors()
            .map(|error| (error.code, error.message.clone()))
            .collect::<Vec<_>>()
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

#[test]
fn reaction_network_parses_species_and_reactions() {
    let source = "\
emath reaction_network HydrogenCombustion:
    species:
        H2
        O2
        H2O
    reactions:
        r1: 2H2 + O2 -> 2H2O
";
    let statements = declaration_body(source);
    assert!(
        statements
            .iter()
            .any(|kind| matches!(kind, StmtKind::Reaction { .. })),
        "a reaction line must parse as StmtKind::Reaction, got {statements:?}"
    );
}

#[test]
fn stoichiometric_coefficients_are_pairs_not_token_surgery() {
    let source = "\
emath reaction_network Combustion:
    species:
        H2
        O2
        H2O
    reactions:
        r1: 2H2 + O2 -> 2H2O
";
    let statements = declaration_body(source);
    let (lhs, rhs): (&Vec<ReactionTerm>, &Vec<ReactionTerm>) = statements
        .iter()
        .find_map(|kind| match kind {
            StmtKind::Reaction { lhs, rhs, .. } => Some((lhs, rhs)),
            _ => None,
        })
        .expect("reaction line must lower to StmtKind::Reaction");
    assert_eq!(lhs.len(), 2, "two LHS terms");
    assert_eq!(lhs[0].coefficient, 2);
    assert_eq!(lhs[0].species, "H2");
    assert_eq!(lhs[1].coefficient, 1);
    assert_eq!(lhs[1].species, "O2");
    assert_eq!(rhs.len(), 1);
    assert_eq!(rhs[0].coefficient, 2);
    assert_eq!(rhs[0].species, "H2O");
}

#[test]
fn arrow_kinds_are_distinct() {
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
    let statements = declaration_body(source);
    let arrows: Vec<ReactionArrow> = statements
        .iter()
        .filter_map(|kind| match kind {
            StmtKind::Reaction { arrow, .. } => Some(*arrow),
            _ => None,
        })
        .collect();
    assert_eq!(arrows.len(), 3, "three reaction lines, got {arrows:?}");
    assert!(matches!(arrows[0], ReactionArrow::Irreversible));
    assert!(matches!(arrows[1], ReactionArrow::Reversible));
    assert!(matches!(arrows[2], ReactionArrow::Equilibrium));
}

#[test]
fn lambda_arrow_is_token_identical_to_irreversible() {
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
    let statements = declaration_body(source);
    let arrows: Vec<ReactionArrow> = statements
        .iter()
        .filter_map(|kind| match kind {
            StmtKind::Reaction { arrow, .. } => Some(*arrow),
            _ => None,
        })
        .collect();
    assert_eq!(arrows, vec![ReactionArrow::Irreversible]);
}

/// `<==>` is the logical Iff token, not a reaction arrow: refuses E-SYN-156.
#[test]
fn iff_arrow_is_refused_inside_reactions() {
    let fixture = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/invalid/reaction_lambda_arrow.emath"
    ));
    assert!(
        fixture.contains("expect: E-SYN-156"),
        "fixture must pin E-SYN-156"
    );
    let (_tree, diagnostics) = parse_str(fixture);
    assert!(
        diagnostics.errors().any(|error| error.code == "E-SYN-156"),
        "`<==>` inside `reactions:` must refuse E-SYN-156, got {:?}",
        diagnostics
            .errors()
            .map(|error| (error.code, error.message.clone()))
            .collect::<Vec<_>>()
    );
}

/// Trailing tokens after the products refuse E-SYN-156: the line ends
/// after the RHS terms, and nothing is silently truncated.
#[test]
fn trailing_tokens_after_products_are_refused() {
    let fixture = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/invalid/reaction_trailing_tokens.emath"
    ));
    assert!(
        fixture.contains("expect: E-SYN-156"),
        "fixture must pin E-SYN-156"
    );
    let (_tree, diagnostics) = parse_str(fixture);
    assert!(
        diagnostics.errors().any(|error| error.code == "E-SYN-156"),
        "trailing tokens after products must refuse E-SYN-156, got {:?}",
        diagnostics
            .errors()
            .map(|error| (error.code, error.message.clone()))
            .collect::<Vec<_>>()
    );
}
