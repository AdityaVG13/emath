//! emath-r3-equilibrium-ds6x failure-first tests (04 §3.3; builds on the
//! CLOSED emath-r3-reactions-section-92hq grammar).
//!
//! Contracts (each must FAIL against the pre-bead admission):
//! - an equilibrium constant line (`Ka: Measured<Real> in M = …`) is
//!   admitted in a `reaction_network` body and carries its uncertainty;
//!   a Ka without the uncertainty form refuses (E-CHEM-KA-EXACT) —
//!   uncertainty is the point of a measured equilibrium constant.
//! - `K == kf/kr` honesty triangle: a network declaring BOTH a reversible
//!   kinetic pair (`<->` with a rate) AND an equilibrium (`<=>` with a
//!   constant) must be consistent within combined uncertainty; violation
//!   = `E-CHEM-THERMO`. Without both sides, the gate stays silent.
//! - the `<=>` relation is RECORDED as recognized meaning (admission
//!   trace), never evaluated — Newton solving is the eval tier, fenced.

use emath_core::limits::Limits;
use emath_sema::CompilerSession;
use emath_syntax::install_source_parser;

fn check(source: &str) -> Vec<(String, String)> {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    session
        .check_owned("equilibrium", source)
        .diagnostics
        .items()
        .iter()
        .map(|diagnostic| (format!("{:?}", diagnostic.severity), diagnostic.code.to_string()))
        .collect()
}

fn errors(out: &[(String, String)]) -> Vec<&(String, String)> {
    out.iter().filter(|(severity, _)| severity == "Error").collect()
}

const ACETIC: &str = "\
emath reaction_network AceticDissociation:
    species:
        CH3COOH
        H2O
        CH3COO
        H3O
    Ka: Measured<Real> in M = 1.75(3)e-5
    reactions:
        dissoc: CH3COOH + H2O <=> CH3COO + H3O
";

/// The bead's flagship: acetic acid dissociation with a measured Ka
/// admits once the constant-line form exists.
#[test]
fn equilibrium_constant_line_admits() {
    let out = check(ACETIC);
    assert!(
        errors(&out).is_empty(),
        "measured Ka line must admit, got {:?}",
        out
    );
}

/// Negative control: an equilibrium constant without uncertainty is a
/// refusal — a measured Ka is uncertain by nature; an exact literal is
/// the dishonest spelling.
#[test]
fn ka_without_uncertainty_refuses() {
    let fixture = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/invalid/equilibrium_ka_specification.emath"
    ));
    assert!(
        fixture.contains("expect: E-CHEM-KA-EXACT"),
        "fixture must pin E-CHEM-KA-EXACT"
    );
    let out = check(fixture);
    let errs = errors(&out);
    assert!(
        errs.iter().any(|(_, code)| code == "E-CHEM-KA-EXACT"),
        "Ka without uncertainty must refuse E-CHEM-KA-EXACT, got {errs:?}"
    );
}

/// Flagship negative control: a kinetic pair (`<->` with kf/kr) plus an
/// equilibrium (`<=>` with K) whose K is inconsistent with kf/kr refuses
/// E-CHEM-THERMO.
#[test]
fn inconsistent_k_vs_kf_kr_refuses_e_chem_thermo() {
    let fixture = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/invalid/equilibrium_thermodynamics.emath"
    ));
    assert!(
        fixture.contains("expect: E-CHEM-THERMO"),
        "fixture must pin E-CHEM-THERMO"
    );
    let out = check(fixture);
    let errs = errors(&out);
    assert!(
        errs.iter().any(|(_, code)| code == "E-CHEM-THERMO"),
        "inconsistent K vs kf/kr must refuse E-CHEM-THERMO, got {errs:?}"
    );
}

/// Honesty triangle, consistent side: with K == kf/kr within uncertainty
/// the network admits — the gate must not fire false positives.
#[test]
fn consistent_k_with_rates_admits() {
    let out = check(
        "emath reaction_network ConsistentPair:\n    species:\n        A\n        B\n    K: Measured<Real> = 2.0(1)\n    rate:\n        kf = 4.0\n        kr = 2.0\n    reactions:\n        kinetic: A <-> B\n        equil: A <=> B\n",
    );
    assert!(
        errors(&out).is_empty(),
        "consistent K == kf/kr must admit, got {:?}",
        out
    );
}

/// Honesty triangle, missing-constant arm: a reversible pair plus an
/// equilibrium with NO constant line at all refuses E-CHEM-THERMO — the
/// consistency claim must be declared, never left unstated.
#[test]
fn missing_k_with_both_arrows_refuses_e_chem_thermo() {
    let fixture = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/invalid/equilibrium_missing_constant.emath"
    ));
    assert!(
        fixture.contains("expect: E-CHEM-THERMO"),
        "fixture must pin E-CHEM-THERMO"
    );
    let out = check(fixture);
    let errs = errors(&out);
    assert!(
        errs.iter().any(|(_, code)| code == "E-CHEM-THERMO"),
        "both arrows without a declared K must refuse E-CHEM-THERMO, got {errs:?}"
    );
}
