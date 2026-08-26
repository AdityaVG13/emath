//! Public-API integration tests for the emath-term crate.
//!
//! Exercises only the published surface: `Signature`, `Term`,
//! `Term::canonical` / `Term::parse_canonical` round-trip, and the typed
//! `TermError` / `CanonicalError` refusals.

use emath_term::{CanonicalError, Signature, SymbolId, Term, TermError, VariableId};

fn sample_term() -> Term {
    Term::Apply {
        operator: SymbolId("mul".to_string()),
        arguments: vec![
            Term::Constant(SymbolId("a".to_string())),
            Term::Apply {
                operator: SymbolId("add".to_string()),
                arguments: vec![
                    Term::Variable(VariableId("x".to_string())),
                    Term::Constant(SymbolId("b".to_string())),
                ],
            },
        ],
    }
}

fn assert_round_trip(term: Term) {
    let canonical = term.canonical();
    let parsed = Term::parse_canonical(&canonical)
        .unwrap_or_else(|err| panic!("parse failed for {canonical:?}: {err:?}"));
    assert_eq!(parsed, term, "parse(canonical(t)) != t for {canonical}");
    assert_eq!(
        parsed.canonical(),
        canonical,
        "canonical not stable for {canonical}"
    );
}

fn var(name: &str) -> Term {
    Term::Variable(VariableId(name.to_string()))
}

fn constant(name: &str) -> Term {
    Term::Constant(SymbolId(name.to_string()))
}

fn apply(op: &str, arguments: Vec<Term>) -> Term {
    Term::Apply {
        operator: SymbolId(op.to_string()),
        arguments,
    }
}

#[test]
fn canonical_round_trip_is_byte_exact() {
    let term = sample_term();
    let canonical = term.canonical();
    let parsed = Term::parse_canonical(&canonical).expect("canonical form must re-parse");
    assert_eq!(parsed, term);
    assert_eq!(parsed.canonical(), canonical);
}

/// G2 identity on constructors the ASCII sample term never built.
#[test]
fn canonical_round_trip_untested_constructors() {
    assert_round_trip(constant("ζ"));
    assert_round_trip(apply("ζ", vec![]));
    assert_round_trip(var("("));
    assert_round_trip(var("a,b"));
    assert_round_trip(var("\\n"));
    assert_round_trip(apply("f(g,h)", vec![constant("ζ")]));
    assert_round_trip(apply(
        "⊛",
        vec![
            apply("⧖", vec![apply("⋈", vec![var("a"), var("b")])]),
            constant("ζ"),
        ],
    ));
    assert_round_trip(apply(
        "apply",
        vec![apply("f", vec![]), apply("const", vec![constant("a")])],
    ));
}

#[test]
fn nested_looking_apply_is_not_flattened_into_an_operator_name() {
    // `apply(const(ζ)` looks like apply wrapping const(ζ). parse_name used
    // to treat unescaped `(` as a name character, so this succeeded as
    // Apply { operator: "const(ζ", arguments: [] } — nested structure lost.
    assert!(matches!(
        Term::parse_canonical("apply(const(ζ)"),
        Err(CanonicalError::Malformed { .. })
    ));
    assert!(matches!(
        Term::parse_canonical("apply(var(x)"),
        Err(CanonicalError::Malformed { .. })
    ));
    // Unknown escapes used to drop the backslash (`var(\n)` → var(n)).
    assert!(matches!(
        Term::parse_canonical("var(\\n)"),
        Err(CanonicalError::Malformed { .. })
    ));
    assert!(matches!(
        Term::parse_canonical("const(\\ζ)"),
        Err(CanonicalError::Malformed { .. })
    ));
    assert!(matches!(
        Term::parse_canonical("var(a,b)"),
        Err(CanonicalError::Malformed { .. })
    ));
    // The honest nested and escaped forms still round-trip.
    assert_round_trip(apply("f", vec![constant("ζ")]));
    assert_round_trip(apply("const(ζ", vec![]));
    assert_round_trip(var("a,b"));
    assert_round_trip(var("\\n"));
}

#[test]
fn canonical_round_trip_tolerates_trailing_whitespace() {
    // CONF-0004/0016: `parse_canonical(canonical(t)) == t` must also
    // hold for the canonical string padded with trailing whitespace
    // (the oracle parser skips it). The generated SG copy is pinned to
    // match in `term_oracle_differential.rs`.
    let term = sample_term();
    let canonical = term.canonical();
    let padded = format!("{canonical}  \n\t ");
    let parsed = Term::parse_canonical(&padded).expect("trailing whitespace is tolerated");
    assert_eq!(parsed, term);
    assert_eq!(parsed.canonical(), canonical);
}

#[test]
fn malformed_or_trailing_canonical_is_refused() {
    assert!(matches!(
        Term::parse_canonical("apply("),
        Err(CanonicalError::Malformed { .. })
    ));
    assert!(matches!(
        Term::parse_canonical("const(a) trailing"),
        Err(CanonicalError::Trailing { .. })
    ));
}

#[test]
fn signature_validates_arity_and_conflicts() {
    let mut sig = Signature::default();
    sig.insert(SymbolId("f".to_string()), 2)
        .expect("fresh symbol inserts");
    assert_eq!(sig.arity(&SymbolId("f".to_string())), Some(2));
    assert!(matches!(
        sig.insert(SymbolId("f".to_string()), 3),
        Err(TermError::ConflictingArity { .. })
    ));
    let wrong_arity = Term::Apply {
        operator: SymbolId("f".to_string()),
        arguments: vec![Term::Constant(SymbolId("a".to_string()))],
    };
    assert!(matches!(
        sig.validate(&wrong_arity),
        Err(TermError::ArityMismatch { .. })
    ));
}
