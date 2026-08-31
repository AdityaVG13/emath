//! Compartments and populations thin slice (bead
//! emath-r3-compartments-e5zq, 04 §4.1+§4.2).
//!
//! Contracts:
//! - **Declared sink `∅`**: a reaction endpoint that is deliberately
//!   nothing is the DECLARED sink spelling (`Drug -> ∅`), admitted as
//!   an empty side (lexed as its own token, never glued into a
//!   non-ASCII identifier); a side that is empty WITHOUT the sink
//!   declaration refuses (`E-SYN-156` at parse, `E-BIO-SINK` at
//!   admission) — an endpoint that is nothing must be declared
//!   nothing, never silently empty;
//! - **`compartments:` / `populations:` sections refuse naming their
//!   design forks** (C15 `@`-vs-attribute-sigil collision + separator
//!   alternative; the ODE-vs-SSA two-readings portfolio and the
//!   stochastic-world prerequisite) instead of a generic
//!   unknown-section error;
//! - ordinary reaction networks admit unchanged.
//!
//! Failure-first evidence: live probes before the slice — `∅` glued
//! into an unknown identifier (E-SYN-114 warning + E-SYN-110), an
//! empty-side reaction was unrepresentable, and the two sections died
//! with the generic whitelist/unknown-section errors.

use emath_core::limits::Limits;
use emath_sema::session::CompilerSession;

fn install_source_parser() {
    emath_syntax::install_source_parser();
}

fn check(text: &str, name: &str) -> Vec<String> {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let result = session.check_owned(name, text);
    result
        .diagnostics
        .errors()
        .map(|d| format!("{} {}", d.code, d.message))
        .collect()
}

const SINK_REACTION: &str = "\
emath reaction_network Degradation:
    species:
        Drug

    reactions:
        elimination: Drug -> ∅
";

const EMPTY_SIDE: &str = "\
emath reaction_network Broken:
    species:
        Drug

    reactions:
        elimination: Drug ->
";

const COMPARTMENTS_SECTION: &str = "\
emath reaction_network PK:
    species:
        Drug

    compartments:
        central: Volume = 1.0
";

const POPULATIONS_SECTION: &str = "\
emath reaction_network FishPop:
    species:
        Fish

    populations:
        N: Population of Fish
";

const PLAIN_NETWORK: &str = "\
emath reaction_network Simple:
    species:
        A
        B

    reactions:
        convert: A -> B
";

#[test]
fn declared_sink_reaction_admits() {
    let errors = check(SINK_REACTION, "sink-admit");
    assert!(
        errors.is_empty(),
        "`Drug -> ∅` (the declared sink) must admit; got: {errors:#?}"
    );
}

#[test]
fn empty_side_without_sink_refuses() {
    let errors = check(EMPTY_SIDE, "empty-side-fence");
    assert!(
        errors.iter().any(|e| e.starts_with("E-SYN-156")),
        "a reaction endpoint that is empty without the declared `∅` must \
         refuse (E-SYN-156 at parse); got: {errors:#?}"
    );
}

#[test]
fn sink_glyph_never_glues_into_identifier() {
    let errors = check(SINK_REACTION, "sink-lexer");
    assert!(
        !errors
            .iter()
            .any(|e| e.contains("E-SYN-114") || e.contains("non-ASCII")),
        "the sink glyph must be its own token, never glued into a \
         non-ASCII identifier warning; got: {errors:#?}"
    );
}

#[test]
fn compartments_section_refuses_naming_design_fork() {
    let errors = check(COMPARTMENTS_SECTION, "compartments-fence");
    assert!(
        errors.iter().any(|e| e.contains("compartments")
            && (e.contains("C15") || e.contains("@"))),
        "`compartments:` must refuse naming the C15 `@` collision fork; \
         got: {errors:#?}"
    );
}

#[test]
fn populations_section_refuses_naming_world_fork() {
    let errors = check(POPULATIONS_SECTION, "populations-fence");
    assert!(
        errors.iter().any(|e| e.contains("populations")
            && e.contains("gillespie_exact")),
        "`populations:` must refuse naming the ODE-vs-SSA two-readings \
         design fork and the stochastic-world prerequisite; got: {errors:#?}"
    );
}

#[test]
fn plain_networks_admit_unchanged() {
    let errors = check(PLAIN_NETWORK, "sink-plain-guard");
    assert!(
        errors.is_empty(),
        "the sink slice must not affect ordinary reaction networks; got: \
         {errors:#?}"
    );
}
