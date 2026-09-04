//! admission-side contracts (sema tier).
//!
//! Species closure and element balance are checked at admission, statically:
//! - every species in a reaction line must be declared in `species:`
//!   (world-closing; undeclared = `E-CHEM-SPECIES`, never implicit);
//! - element balance: summed atoms per element must match across the arrow
//!   (imbalance = `E-CHEM-BALANCE`); the balanced combustion line admits.

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
        .check_owned("reactions", source)
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

/// Balanced hydrogen combustion admits: 4 H + 2 O -> 4 H + 2 O.
#[test]
fn balanced_reaction_admits() {
    let out = check(
        "emath reaction_network HydrogenCombustion:\n    species:\n        H2\n        O2\n        H2O\n    reactions:\n        r1: 2H2 + O2 -> 2H2O\n",
    );
    let errors: Vec<&(String, String)> = out
        .iter()
        .filter(|(severity, _)| severity == "Error")
        .collect();
    assert!(
        errors.is_empty(),
        "balanced network must admit, got {errors:?}"
    );
}

/// Negative control: an undeclared species refuses with the pinned code.
#[test]
fn undeclared_species_is_e_chem_species() {
    let fixture = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/invalid/reaction_undeclared_species.emath"
    ));
    assert!(
        fixture.contains("expect: E-CHEM-SPECIES"),
        "fixture must pin E-CHEM-SPECIES"
    );
    let out = check(fixture);
    assert!(
        out.iter()
            .any(|(severity, code)| severity == "Error" && code == "E-CHEM-SPECIES"),
        "undeclared species must refuse E-CHEM-SPECIES, got {out:?}"
    );
}

/// Negative control: element imbalance is a typed admission refusal.
#[test]
fn imbalanced_reaction_is_e_chem_balance() {
    let fixture = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/invalid/reaction_imbalance.emath"
    ));
    assert!(
        fixture.contains("expect: E-CHEM-BALANCE"),
        "fixture must pin E-CHEM-BALANCE"
    );
    let out = check(fixture);
    assert!(
        out.iter()
            .any(|(severity, code)| severity == "Error" && code == "E-CHEM-BALANCE"),
        "imbalance must refuse E-CHEM-BALANCE, got {out:?}"
    );
}

/// Negative control: an unknown body section refuses E-KIND-027 — the
/// closed section table cannot be extended by spelling.
#[test]
fn unknown_section_is_e_kind_027() {
    let fixture = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/invalid/reaction_unknown_section.emath"
    ));
    assert!(
        fixture.contains("expect: E-KIND-027"),
        "fixture must pin E-KIND-027"
    );
    let out = check(fixture);
    assert!(
        out.iter()
            .any(|(severity, code)| severity == "Error" && code == "E-KIND-027"),
        "unknown section must refuse E-KIND-027, got {out:?}"
    );
}

/// Negative control: a two-names-on-one-line `species:` entry is not a
/// bare declaration — closure refuses E-CHEM-SPECIES (and the
/// consequent undeclared-species refusal), it never guesses.
#[test]
fn nonbare_species_entry_is_e_chem_species() {
    let fixture = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/invalid/reaction_nonbare_species.emath"
    ));
    assert!(
        fixture.contains("expect: E-CHEM-SPECIES"),
        "fixture must pin E-CHEM-SPECIES"
    );
    let out = check(fixture);
    assert!(
        out.iter()
            .any(|(severity, code)| severity == "Error" && code == "E-CHEM-SPECIES"),
        "non-bare species entry must refuse E-CHEM-SPECIES, got {out:?}"
    );
}
