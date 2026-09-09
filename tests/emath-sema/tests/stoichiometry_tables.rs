//! admission-side contracts (sema tier).
//!
//! The anti-transcription-error design: stoichiometric coefficients are
//! DERIVED from the declared reaction lines, never re-entered freely.

use emath_test_harness::{boot, Probe, Source};

const BASE: &str = "emath reaction_network ProbeNet:\n    species:\n        A\n        B\n    reactions:\n        r1: A -> B\n";

const ICE: &str = "    ice_table r1:\n        initial:\n            A = 1.0\n            B = 0.0\n        change:\n            A = -1\n            B = 1\n";

#[test]
fn stoichiometry_contract() {
    boot();
    let mut p = Probe::new("stoichiometry derives from reaction lines, never re-entered");
    p.case("derived-matrix", |p| {
        Source::from_str(
            "stoich-derive",
            &format!("{BASE}    stoichiometry:\n        nu = stoich(reactions)\n"),
        )
        .must_admit(p);
    });
    p.case("rhs-derived-call-only", |p| {
        Source::from_workspace("tests/invalid/stoichiometry_rhs.emath")
            .must_refuse(p, &["E-CHEM-STOICH"]);
    });
    p.case("ice-matching-row", |p| {
        Source::from_str("ice-ok", &format!("{BASE}{ICE}")).must_admit(p);
    });
    p.case("ice-change-mismatch", |p| {
        Source::from_workspace("tests/invalid/stoichiometry_change_mismatch.emath")
            .must_refuse(p, &["E-CHEM-STOICH"]);
    });
    p.case("ice-bystander", |p| {
        Source::from_workspace("tests/invalid/stoichiometry_bystander_species.emath")
            .must_refuse(p, &["E-CHEM-STOICH"]);
    });
    p.case("ice-missing-entry", |p| {
        Source::from_workspace("tests/invalid/stoichiometry_missing_entry.emath")
            .must_refuse(p, &["E-CHEM-STOICH"]);
    });
    p.case("ice-unknown-reaction", |p| {
        Source::from_workspace("tests/invalid/stoichiometry_unknown_reaction.emath")
            .must_refuse(p, &["E-CHEM-STOICH"]);
    });
    p.case("typed-extent", |p| {
        Source::from_str(
            "extent",
            &format!("{BASE}    extents:\n        xi: Real in mol\n{ICE}"),
        )
        .must_admit(p);
    });
    p.case("equilibrium-identity", |p| {
        Source::from_str(
            "equilibrium-ok",
            &format!(
                "{BASE}    extents:\n        xi: Real in mol\n{ICE}        equilibrium = initial + xi * change\n"
            ),
        )
        .must_admit(p);
    });
    p.case("equilibrium-nonidentity", |p| {
        Source::from_workspace("tests/invalid/stoichiometry_equilibrium_nonidentity.emath")
            .must_refuse(p, &["E-CHEM-STOICH"]);
    });
    p.case("forall-constraint", |p| {
        Source::from_str(
            "forall",
            &format!("{BASE}    constraints:\n        forall s in species: 0 M <= [s]\n"),
        )
        .must_admit(p);
    });
    p.case("malformed-constraint", |p| {
        Source::from_workspace("tests/invalid/stoichiometry_constraints_malformed.emath")
            .must_refuse(p, &["E-KIND-027"]);
    });
    p.case("full-model", |p| {
        Source::from_str(
            "full",
            &format!(
                "{BASE}    stoichiometry:\n        nu = stoich(reactions)\n    extents:\n        xi: Real in mol\n{ICE}        equilibrium = initial + xi * change\n    constraints:\n        forall s in species: 0 M <= [s]\n"
            ),
        )
        .must_admit(p);
    });
    p.case("combustion-balances", |p| {
        Source::from_str(
            "combustion",
            "emath reaction_network HydrogenCombustion:\n    species:\n        H2\n        O2\n        H2O\n    reactions:\n        combustion: 2H2 + O2 -> 2H2O\n    stoichiometry:\n        nu = stoich(reactions)\n",
        )
        .must_admit(p);
    });
    p.finish();
}
