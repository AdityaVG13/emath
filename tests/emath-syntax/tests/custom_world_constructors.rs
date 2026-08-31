//! `emath-nko`: custom world constructor levels (value / world / artifact).
//!
//! The three constructor levels from spec 09:
//! - value constructor: builds a valid value inside a world (exists on
//!   HEAD as ordinary `constructors:` admission);
//! - world constructor: `world constructor <name>:` declares strategies
//!   and outputs an interpretation portfolio — bounded, deterministic,
//!   evidence-authority-neutral;
//! - artifact constructor: `artifact constructor <name>:` packages a
//!   selected world into software; the Phase 1 subset refuses it rather
//!   than silently accepting a construct it cannot implement.
//!
//! Expansion safety: determinism (same source → same identity), no
//! evidence minting, and refusal of unimplemented lowering.

use emath_core::limits::Limits;
use emath_ir::{ClaimVerdict, EvidenceLevel};
use emath_sema::CompilerSession;
use emath_syntax::{install_source_parser, parse_str};

fn check(name: &str, source: &str) -> emath_sema::admit::CheckResult {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    session.check_owned(name, source)
}

#[test]
fn nko_value_constructor_level_admits_deterministically() {
    // Value level (spec 09): a constructor that validates and builds.
    // `emath policy` is the stateful value-constructor lane on HEAD.
    let source = "\
emath policy Probability:
    inputs:
        x: Float64
    outputs:
        p: Float64
    state:
        value: Float64
    constructors:
        public fn new(x: Float64) -> Result<Self, RangeError>:
            require 0 <= x
            require x <= 1
            Self:
                value = x
    definitions:
        p = state.value
";
    let (_, parse_diagnostics) = parse_str(source);
    assert!(!parse_diagnostics.has_errors());
    let checked = check("value-level", source);
    assert!(
        !checked.diagnostics.has_errors(),
        "{:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    );
    assert_eq!(checked.package.declarations[0].constructors.len(), 1);

    let repeated = check("value-level-repeat", source);
    assert_eq!(
        checked.package.meaning_id(&[]).unwrap(),
        repeated.package.meaning_id(&[]).unwrap(),
        "custom expansion must be deterministic (same source, same identity)"
    );
}

#[test]
fn nko_world_constructor_level_admits_with_labeled_portfolio_output() {
    // World level (spec 09): strategies + protect + portfolio output.
    // Deterministic, evidence-neutral: the declaration never mints a
    // claim higher than E1/not-run.
    let source = "\
emath custom AlienWorld:
    world constructor invent:
        strategies:
            free_symbolic
            finite_table
        protect:
            total
            deterministic
        output: \"InterpretationPortfolio\"
";
    let (_, parse_diagnostics) = parse_str(source);
    assert!(!parse_diagnostics.has_errors());
    let checked = check("world-level", source);
    assert!(
        !checked.diagnostics.has_errors(),
        "{:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    );
    let world = &checked.package.declarations[0];
    assert_eq!(world.kind_label, "custom");
    assert_eq!(world.evidence.len(), 1);
    assert_eq!(world.evidence[0].verdict, ClaimVerdict::NotRun);
    assert_eq!(world.evidence[0].level, EvidenceLevel::E1);
    assert_eq!(world.evidence[0].checker, None);

    let repeated = check("world-level-repeat", source);
    assert_eq!(
        checked.package.meaning_id(&[]).unwrap(),
        repeated.package.meaning_id(&[]).unwrap()
    );
}

#[test]
fn nko_artifact_constructor_refused_until_implemented() {
    // Artifact level (spec 09): packaging is not a Phase 1 capability.
    // "Do not silently accept a custom construct the Phase 1 subset does
    // not implement" — the refusal must be typed, not a crash.
    let source = "\
emath custom RustWorld:
    artifact constructor rust_component:
        include:
            evaluator
";
    let checked = check("artifact-level", source);
    assert!(
        checked.diagnostics.has_errors(),
        "artifact constructor must be refused in Phase 1"
    );
}

#[test]
fn nko_forbidden_expansion_refuses() {
    // Expansion safety: a custom declaration cannot mint evidence
    // authority by declaration alone (invalid fixture, typed refusal).
    let invalid = check(
        "invalid-nko",
        include_str!("../../../tests/invalid/custom_world_missing_witness.emath"),
    );
    assert!(
        invalid
            .diagnostics
            .errors()
            .any(|error| error.code == "E-KIND-027"),
        "{:?}",
        invalid.diagnostics.errors().collect::<Vec<_>>()
    );
    assert!(invalid.package.declarations.is_empty());
}
