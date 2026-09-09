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

use emath_ir::{ClaimVerdict, EvidenceLevel};
use emath_syntax::{parse_str};

fn check(name: &str, source: &str) -> emath_sema::admit::CheckResult {
    Source::from_str(name, source).check()
}

use emath_test_harness::{Probe, Source, boot};

#[test]
fn custom_world_constructors() {
    boot();
    let mut probe = Probe::new("`emath-nko`: custom world constructor levels (value / world / artifact). The three constructor levels from spec 09: - value constructor: builds a");
    probe.case("nko_value_constructor_level_admits_deterministically", |p| {
    let f0 = p.failures().len();

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
    p.demand("1",!parse_diagnostics.has_errors(), stringify!(!parse_diagnostics.has_errors()));
    if p.failures().len() != f0 { return; }
    let checked = check("value-level", source);
    p.demand("2",!checked.diagnostics.has_errors(), format!(
        "{:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    p.eq("3", checked.package.declarations[0].constructors.len(), 1);

    let repeated = check("value-level-repeat", source);
    p.eq("4", checked.package.meaning_id(&[]).unwrap(), repeated.package.meaning_id(&[]).unwrap());

    });
    probe.case("nko_world_constructor_level_admits_with_labeled_portfolio_output", |p| {
    let f0 = p.failures().len();

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
    p.demand("1",!parse_diagnostics.has_errors(), stringify!(!parse_diagnostics.has_errors()));
    if p.failures().len() != f0 { return; }
    let checked = check("world-level", source);
    p.demand("2",!checked.diagnostics.has_errors(), format!(
        "{:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let world = &checked.package.declarations[0];
    p.demand("3", (world.kind_label) == ("custom"), format!("expected {:?}, got {:?}", ("custom"), (world.kind_label)));
    if p.failures().len() != f0 { return; }
    p.eq("4", world.evidence.len(), 1);
    p.eq("5", world.evidence[0].verdict, ClaimVerdict::NotRun);
    p.eq("6", world.evidence[0].level, EvidenceLevel::E1);
    p.demand("7", world.evidence[0].checker == None, format!("expected None, got {:?}", (world.evidence[0].checker)));
    if p.failures().len() != f0 { return; }

    let repeated = check("world-level-repeat", source);
    p.eq("8", checked.package.meaning_id(&[]).unwrap(), repeated.package.meaning_id(&[]).unwrap());

    });
    probe.case("nko_artifact_constructor_refused_until_implemented", |p| {
    let f0 = p.failures().len();

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
    p.demand("1",checked.diagnostics.has_errors(), format!(
        "artifact constructor must be refused in Phase 1"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("nko_forbidden_expansion_refuses", |p| {
    let f0 = p.failures().len();

    // Expansion safety: a custom declaration cannot mint evidence
    // authority by declaration alone (invalid fixture, typed refusal).
    let invalid = check(
        "invalid-nko",
        include_str!("../../../tests/invalid/custom_world_missing_witness.emath"),
    );
    p.demand("1",invalid
            .diagnostics
            .errors()
            .any(|error| error.code == "E-KIND-027"), format!(
        "{:?}",
        invalid.diagnostics.errors().collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    p.demand("2",invalid.package.declarations.is_empty(), stringify!(invalid.package.declarations.is_empty()));
    if p.failures().len() != f0 { return; }

    });
    probe.finish();
}
