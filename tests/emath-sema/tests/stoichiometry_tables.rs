//! admission-side contracts (sema tier).
//!
//! The anti-transcription-error design: stoichiometric coefficients are
//! DERIVED from the declared reaction lines, never re-entered freely.
//! - `stoichiometry:` derives a matrix; the only admitted right-hand side
//!   is exactly `stoich(reactions)` (anything else = E-CHEM-STOICH).
//! - `ice_table <reaction>:` rows are checked against the derived ν:
//!   change entries must match the reaction's coefficients (mismatch,
//!   bystander species, missing species = E-CHEM-STOICH).
//! - `extents:` declares typed extents of reaction (`xi: Real in mol`).
//! - `equilibrium = initial + xi * change` is admitted as the derived
//!   identity — any other formula refuses (re-entered equilibrium rows
//!   are the classic transcription error).
//! - `constraints:` admits forall-over-species concentration claims.

use emath_core::limits::Limits;
use emath_sema::CompilerSession;
use emath_syntax::install_source_parser;

fn check(source: &str) -> Vec<(String, String)> {
    {
        // Capsule admission resolves only through the installed language
        // distribution; install per thread before any session (rat_cells pattern).
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../language");
        let distribution = emath_exec_ir::language_image::load_language_distribution(&root)
            .expect("load capsule distribution");
        emath_sema::language::install_language_distribution(&distribution)
            .expect("install capsule-active kernels");
    }
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    session
        .check_owned("stoich", source)
        .diagnostics
        .items()
        .iter()
        .map(|diagnostic| {
            (
                format!("{:?}", diagnostic.severity),
                diagnostic.code.to_string(),
            )
        })
        .collect()
}

fn errors_of(out: &[(String, String)]) -> Vec<&(String, String)> {
    out.iter()
        .filter(|(severity, _)| severity == "Error")
        .collect()
}

const BASE: &str = "emath reaction_network ProbeNet:\n    species:\n        A\n        B\n    reactions:\n        r1: A -> B\n";

/// The derived stoichiometric matrix admits: `nu = stoich(reactions)` is
/// the exact spelling; the matrix is computed from the reaction lines,
/// never re-entered.
#[test]
fn stoich_matrix_derives_and_admits() {
    let out = check(&format!(
        "{BASE}    stoichiometry:\n        nu = stoich(reactions)\n"
    ));
    assert!(
        errors_of(&out).is_empty(),
        "derived stoichiometry must admit, got {out:?}"
    );
}

/// The only admitted right-hand side is exactly `stoich(reactions)` — a
/// re-entered or invented matrix defeats the anti-transcription design.
#[test]
fn stoich_rhs_must_be_the_derived_call() {
    let fixture = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/invalid/stoichiometry_rhs.emath"
    ));
    assert!(
        fixture.contains("expect: E-CHEM-STOICH"),
        "fixture must pin E-CHEM-STOICH"
    );
    let out = check(fixture);
    assert!(
        out.iter()
            .any(|(severity, code)| severity == "Error" && code == "E-CHEM-STOICH"),
        "non-derived stoichiometry RHS must refuse E-CHEM-STOICH, got {out:?}"
    );
}

/// An ICE table whose change row matches the derived coefficients admits.
#[test]
fn ice_table_matching_change_row_admits() {
    let src = format!(
        "{BASE}    ice_table r1:\n        initial:\n            A = 1.0\n            B = 0.0\n        change:\n            A = -1\n            B = 1\n"
    );
    let out = check(&src);
    assert!(
        errors_of(&out).is_empty(),
        "ICE table with correct change row must admit, got {out:?}"
    );
}

/// Negative control: a re-entered change coefficient that disagrees with
/// the reaction line refuses E-CHEM-STOICH (the transcription error the
/// exists to catch).
#[test]
fn ice_table_change_mismatch_is_e_chem_stoich() {
    let fixture = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/invalid/stoichiometry_change_mismatch.emath"
    ));
    assert!(
        fixture.contains("expect: E-CHEM-STOICH"),
        "fixture must pin E-CHEM-STOICH"
    );
    let out = check(fixture);
    assert!(
        out.iter()
            .any(|(severity, code)| severity == "Error" && code == "E-CHEM-STOICH"),
        "change-coefficient mismatch must refuse E-CHEM-STOICH, got {out:?}"
    );
}

/// A change entry for a species the reaction does not involve is a
/// transcription error: E-CHEM-STOICH, never silently ignored.
#[test]
fn ice_table_bystander_species_is_e_chem_stoich() {
    let fixture = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/invalid/stoichiometry_bystander_species.emath"
    ));
    assert!(
        fixture.contains("expect: E-CHEM-STOICH"),
        "fixture must pin E-CHEM-STOICH"
    );
    let out = check(fixture);
    assert!(
        out.iter()
            .any(|(severity, code)| severity == "Error" && code == "E-CHEM-STOICH"),
        "bystander ICE species must refuse E-CHEM-STOICH, got {out:?}"
    );
}

/// An ICE table missing an entry for a reaction species is incomplete:
/// E-CHEM-STOICH (the table must cover the reaction's full carrier).
#[test]
fn ice_table_missing_entry_is_e_chem_stoich() {
    let fixture = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/invalid/stoichiometry_missing_entry.emath"
    ));
    assert!(
        fixture.contains("expect: E-CHEM-STOICH"),
        "fixture must pin E-CHEM-STOICH"
    );
    let out = check(fixture);
    assert!(
        out.iter()
            .any(|(severity, code)| severity == "Error" && code == "E-CHEM-STOICH"),
        "missing ICE entry must refuse E-CHEM-STOICH, got {out:?}"
    );
}

/// An ICE table naming a reaction that does not exist is a typo, not a
/// table: E-CHEM-STOICH (out-of-range references are Err, never false).
#[test]
fn ice_table_unknown_reaction_is_e_chem_stoich() {
    let fixture = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/invalid/stoichiometry_unknown_reaction.emath"
    ));
    assert!(
        fixture.contains("expect: E-CHEM-STOICH"),
        "fixture must pin E-CHEM-STOICH"
    );
    let out = check(fixture);
    assert!(
        out.iter()
            .any(|(severity, code)| severity == "Error" && code == "E-CHEM-STOICH"),
        "ICE table for unknown reaction must refuse E-CHEM-STOICH, got {out:?}"
    );
}

/// `extents:` declares typed extents of reaction (`xi: Real in mol`) and
/// admits alongside the ICE table.
#[test]
fn extents_section_admits_typed_extent() {
    let src = format!(
        "{BASE}    extents:\n        xi: Real in mol\n    ice_table r1:\n        initial:\n            A = 1.0\n            B = 0.0\n        change:\n            A = -1\n            B = 1\n"
    );
    let out = check(&src);
    assert!(
        errors_of(&out).is_empty(),
        "typed extent + ICE table must admit, got {out:?}"
    );
}

/// The equilibrium row is the derived identity `initial + xi * change` —
/// admitted verbatim.
#[test]
fn equilibrium_identity_admits() {
    let src = format!(
        "{BASE}    extents:\n        xi: Real in mol\n    ice_table r1:\n        initial:\n            A = 1.0\n            B = 0.0\n        change:\n            A = -1\n            B = 1\n        equilibrium = initial + xi * change\n"
    );
    let out = check(&src);
    assert!(
        errors_of(&out).is_empty(),
        "equilibrium identity row must admit, got {out:?}"
    );
}

/// Negative control: a re-entered equilibrium formula (not the derived
/// identity) refuses E-CHEM-STOICH.
#[test]
fn equilibrium_nonidentity_is_e_chem_stoich() {
    let fixture = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/invalid/stoichiometry_equilibrium_nonidentity.emath"
    ));
    assert!(
        fixture.contains("expect: E-CHEM-STOICH"),
        "fixture must pin E-CHEM-STOICH"
    );
    let out = check(fixture);
    assert!(
        out.iter()
            .any(|(severity, code)| severity == "Error" && code == "E-CHEM-STOICH"),
        "non-identity equilibrium row must refuse E-CHEM-STOICH, got {out:?}"
    );
}

/// `constraints:` admits forall-over-species concentration claims.
#[test]
fn constraints_forall_species_admits() {
    let src = format!("{BASE}    constraints:\n        forall s in species: 0 M <= [s]\n");
    let out = check(&src);
    assert!(
        errors_of(&out).is_empty(),
        "forall-over-species constraint must admit, got {out:?}"
    );
}

/// Negative control: a malformed constraint (no forall-over-species
/// binder) refuses E-KIND-027 — the closed shape table cannot be
/// extended by spelling.
#[test]
fn constraints_malformed_is_e_kind_027() {
    let fixture = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/invalid/stoichiometry_constraints_malformed.emath"
    ));
    assert!(
        fixture.contains("expect: E-KIND-027"),
        "fixture must pin E-KIND-027"
    );
    let out = check(fixture);
    assert!(
        out.iter()
            .any(|(severity, code)| severity == "Error" && code == "E-KIND-027"),
        "malformed constraint must refuse E-KIND-027, got {out:?}"
    );
}

/// The full -shaped model admits end-to-end: derived matrix, typed
/// extent, ICE table with identity row, concentration constraint.
#[test]
fn full_stoich_model_admits() {
    let src = format!(
        "{BASE}    stoichiometry:\n        nu = stoich(reactions)\n    extents:\n        xi: Real in mol\n    ice_table r1:\n        initial:\n            A = 1.0\n            B = 0.0\n        change:\n            A = -1\n            B = 1\n        equilibrium = initial + xi * change\n    constraints:\n        forall s in species: 0 M <= [s]\n"
    );
    let out = check(&src);
    assert!(
        errors_of(&out).is_empty(),
        "full stoich model must admit, got {out:?}"
    );
}

/// The derived matrix honors real stoichiometric coefficients over
/// element-formula species (2H2 + O2 -> 2H2O must balance AND derive).
#[test]
fn combustion_network_with_stoich_admits() {
    let src = "emath reaction_network HydrogenCombustion:\n    species:\n        H2\n        O2\n        H2O\n    reactions:\n        combustion: 2H2 + O2 -> 2H2O\n    stoichiometry:\n        nu = stoich(reactions)\n";
    let out = check(src);
    assert!(
        errors_of(&out).is_empty(),
        "combustion + derived stoichiometry must admit, got {out:?}"
    );
}
