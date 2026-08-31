//! emath-r3-chem-surface-i6ri failure-first tests (04 §3.4 + §3.5; the
//! §3.6 `record … where` surface needs a cross-lane Declaration change and
//! is intentionally out of this test file).
//!
//! Contracts (each must FAIL against the pre-bead admission):
//! - §3.5: a named rate-law form (`v = michaelis_menten(Vmax, Km, [S])`)
//!   is admitted in a `rate:` section. The form is non-mass-action, so
//!   without a declared `assumptions:` section it carries a WARNING
//!   receipt (W-CHEM-RATELAW) — never a silent admit, never a refusal.
//! - §3.5: `assumptions: quasi_steady_state` is admitted as a declared
//!   approximation; with it present the warning receipt stays silent.
//! - §3.4 (context-scoped minimal): `[S]` inside rate-law arguments is
//!   the concentration-of-S reading when S is a declared species; an
//!   undeclared `[Q]` inside the rate context refuses E-NOTATION-AMBIG
//!   (outside `rate:`/reaction contexts `[x]` stays the list/index
//!   reading — the parser is untouched, so no other suite changes).

use emath_core::limits::Limits;
use emath_sema::CompilerSession;
use emath_syntax::install_source_parser;

fn check(source: &str) -> Vec<(String, String)> {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    session
        .check_owned("chem_surface", source)
        .diagnostics
        .items()
        .iter()
        .map(|diagnostic| (format!("{:?}", diagnostic.severity), diagnostic.code.to_string()))
        .collect()
}

fn errors(out: &[(String, String)]) -> Vec<&(String, String)> {
    out.iter().filter(|(severity, _)| severity == "Error").collect()
}

fn warnings(out: &[(String, String)]) -> Vec<&(String, String)> {
    out.iter().filter(|(severity, _)| severity == "Warning").collect()
}

const MM_PLAIN: &str = "\
emath reaction_network MichaelisMenten:
    species:
        S
        P
        E
        ES
    Vmax: Measured<Real> = 1.0(1)
    Km: Measured<Real> = 0.5(5)
    rate:
        v = michaelis_menten(Vmax, Km, [S])
    reactions:
        cat: ES -> E + P
";

const MM_ASSUMED: &str = "\
emath reaction_network MichaelisMentenAssumed:
    species:
        S
        P
        E
        ES
    Vmax: Measured<Real> = 1.0(1)
    Km: Measured<Real> = 0.5(5)
    rate:
        v = michaelis_menten(Vmax, Km, [S])
    assumptions:
        quasi_steady_state
    reactions:
        cat: ES -> E + P
";

/// §3.5 flagship: the named rate-law form admits; non-mass-action
/// without a declared assumption carries the W-CHEM-RATELAW warning
/// receipt.
#[test]
fn rate_law_form_admits_with_warning_receipt() {
    let out = check(MM_PLAIN);
    assert!(
        errors(&out).is_empty(),
        "named rate-law form must admit, got {:?}",
        out
    );
    assert!(
        warnings(&out)
            .iter()
            .any(|(_, code)| code == "W-CHEM-RATELAW"),
        "non-mass-action rate law without assumptions must warn W-CHEM-RATELAW, got {out:?}"
    );
}

/// With `assumptions: quasi_steady_state` declared the warning stays
/// silent — the approximation is declared, not ambient.
#[test]
fn declared_assumptions_silence_warning() {
    let out = check(MM_ASSUMED);
    assert!(
        errors(&out).is_empty(),
        "declared-assumption network must admit, got {:?}",
        out
    );
    assert!(
        warnings(&out).is_empty(),
        "declared assumptions must silence W-CHEM-RATELAW, got {out:?}"
    );
}

/// §3.4 context-scoped reading: `[S]` with S declared is the
/// concentration reading inside the rate context — no ambiguity refusal.
#[test]
fn bracket_concentration_in_rate_admits() {
    let out = check(MM_PLAIN);
    assert!(
        errors(&out)
            .iter()
            .all(|(_, code)| code != "E-NOTATION-AMBIG"),
        "declared species bracket in rate context must read as concentration, got {out:?}"
    );
}

/// Negative control: an undeclared species inside the rate-context
/// bracket refuses E-NOTATION-AMBIG (no silent guessing).
#[test]
fn bracket_unknown_species_refuses_ambig() {
    let fixture = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/invalid/chemical_rate_law_ambiguous.emath"
    ));
    assert!(
        fixture.contains("expect: E-NOTATION-AMBIG"),
        "fixture must pin E-NOTATION-AMBIG"
    );
    let out = check(fixture);
    let errs = errors(&out);
    assert!(
        errs.iter().any(|(_, code)| code == "E-NOTATION-AMBIG"),
        "undeclared bracket species in rate context must refuse E-NOTATION-AMBIG, got {errs:?}"
    );
}

/// Negative control: a `rate:` entry value that is not a numeric
/// literal refuses E-KIND-027 (rate constants feed the honesty gate;
/// nothing is guessed).
#[test]
fn rate_entry_non_numeric_refuses() {
    let fixture = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/invalid/chemical_rate_nonnumeric.emath"
    ));
    assert!(
        fixture.contains("expect: E-KIND-027"),
        "fixture must pin E-KIND-027"
    );
    let out = check(fixture);
    let errs = errors(&out);
    assert!(
        errs.iter().any(|(_, code)| code == "E-KIND-027"),
        "non-numeric rate entry must refuse E-KIND-027, got {errs:?}"
    );
}

/// Negative control: an `assumptions:` entry that is not a bare name
/// refuses E-KIND-027 (declared approximations hash by name; a
/// call-shaped spelling is not a declaration).
#[test]
fn assumption_entry_malformed_refuses() {
    let fixture = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/invalid/chemical_assumption_malformed.emath"
    ));
    assert!(
        fixture.contains("expect: E-KIND-027"),
        "fixture must pin E-KIND-027"
    );
    let out = check(fixture);
    let errs = errors(&out);
    assert!(
        errs.iter().any(|(_, code)| code == "E-KIND-027"),
        "malformed assumptions entry must refuse E-KIND-027, got {errs:?}"
    );
}
