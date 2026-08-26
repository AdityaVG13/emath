//! Differential Term-oracle pin: generated SG parser vs `emath-term`.
//!
//! CONF-0016 (dual Term parser drift) and CONF-0004 (round-trip pin):
//! the generated parametric crate carries a self-contained canonical
//! parser, which must agree with the `emath-term` oracle on the replay
//! canonical string and on that string plus trailing whitespace (the
//! generated copy skips trailing whitespace exactly like the oracle).
//! The pairing uses the lab `EngineIdentity` type (cluster 4): Subject
//! `generated-sg` vs Oracle `emath-term` — never a second identity
//! module.

use emath_lab_core::{EngineIdentity, EngineRole};

/// The replayed reference term of the parametric worlds crate.
const REPLAY_CANONICAL: &str = "apply(⊛,apply(⧖,apply(⋈,var(a),var(b))),const(ζ))";

#[test]
fn generated_parser_matches_the_oracle_on_the_replay_string() {
    let subject = EngineIdentity {
        role: EngineRole::Subject,
        label: "generated-sg".to_string(),
    };
    let oracle = EngineIdentity {
        role: EngineRole::Oracle,
        label: "emath-term".to_string(),
    };
    subject
        .require_distinct(&oracle, "term oracle differential")
        .expect("subject and oracle must be distinct engine identities");

    let oracle_term = emath_term::Term::parse_canonical(REPLAY_CANONICAL)
        .expect("oracle parses the replay string");
    let generated_term = semantic_genesis_worlds::Term::parse_canonical(REPLAY_CANONICAL)
        .expect("generated copy parses the replay string");
    assert_eq!(generated_term.canonical(), oracle_term.canonical());

    let padded = format!("{REPLAY_CANONICAL}  \n\t ");
    let oracle_padded =
        emath_term::Term::parse_canonical(&padded).expect("oracle tolerates trailing whitespace");
    let generated_padded = semantic_genesis_worlds::Term::parse_canonical(&padded)
        .expect("generated copy must match the oracle on trailing whitespace");
    assert_eq!(generated_padded.canonical(), oracle_padded.canonical());
    assert_eq!(
        generated_padded.canonical(),
        REPLAY_CANONICAL,
        "whitespace tolerance must not change the canonical output"
    );
}

#[test]
fn generated_parser_refuses_garbage_like_the_oracle() {
    assert!(emath_term::Term::parse_canonical("apply(").is_err());
    assert!(semantic_genesis_worlds::Term::parse_canonical("apply(").is_err());
    assert!(emath_term::Term::parse_canonical("const(a) trailing").is_err());
    assert!(semantic_genesis_worlds::Term::parse_canonical("const(a) trailing").is_err());
    // Nested-looking apply without an argument comma must refuse in both
    // copies (unescaped `(` is not a name character).
    assert!(emath_term::Term::parse_canonical("apply(const(ζ)").is_err());
    assert!(semantic_genesis_worlds::Term::parse_canonical("apply(const(ζ)").is_err());
    assert!(emath_term::Term::parse_canonical("var(\\n)").is_err());
    assert!(semantic_genesis_worlds::Term::parse_canonical("var(\\n)").is_err());
}
