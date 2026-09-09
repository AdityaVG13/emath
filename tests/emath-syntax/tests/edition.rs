//! edition tests migrated from the in-crate `#[cfg(test)]` module.

use emath_core::{DeprecationStage, Edition};
use emath_syntax::edition::*;

use emath_test_harness::{Probe, boot};

#[test]
fn edition() {
    boot();
    let mut probe = Probe::new("edition tests migrated from the in-crate `#[cfg(test)]` module.");
    probe.case("edition_2026_selects_grammar_2026_1", |p| {
    let f0 = p.failures().len();

    let profile = grammar_profile_for("2026").expect("2026 ships");
    p.demand("1", (profile.grammar_version) == ("2026.1"), format!("expected {:?}, got {:?}", ("2026.1"), (profile.grammar_version)));
    if p.failures().len() != f0 { return; }
    p.eq("2", profile.edition, Edition::Ed2026);

    });
    probe.case("unknown_edition_is_typed_refusal", |p| {
    let f0 = p.failures().len();

    let Err(error) = grammar_profile_for("2099") else {
        p.demand("1",false, format!( "2099 must not ship"));
        if p.failures().len() != f0 { return; }
        unreachable!();
    };
    p.eq("2", error.code, emath_core::E_PKG_EDITION_UNKNOWN);

    });
    probe.case("recognized_admitted_hidden_not", |p| {
    let f0 = p.failures().len();

    p.demand("1",admitted_by_default(DeprecationStage::Recognized, "2026"), stringify!(admitted_by_default(DeprecationStage::Recognized, "2026")));
    if p.failures().len() != f0 { return; }
    p.demand("2",!admitted_by_default(DeprecationStage::Deprecated, "2026"), stringify!(!admitted_by_default(DeprecationStage::Deprecated, "2026")));
    if p.failures().len() != f0 { return; }
    p.demand("3",!admitted_by_default(DeprecationStage::Hidden, "2026"), stringify!(!admitted_by_default(DeprecationStage::Hidden, "2026")));
    if p.failures().len() != f0 { return; }
    // Frozen is replay-only: never admitted by a default table.
    p.demand("4",!admitted_by_default(DeprecationStage::Frozen, "2026"), stringify!(!admitted_by_default(DeprecationStage::Frozen, "2026")));
    if p.failures().len() != f0 { return; }

    });
    probe.case("grammar_table_covers_every_shipped_edition", |p| {
    let f0 = p.failures().len();

    for edition in Edition::ALL {
        p.demand("1",grammar_profile_for(edition.as_str()).is_ok(), format!(
            "{} unshipped",
            edition
        ));
        if p.failures().len() != f0 { return; }
    }

    });
    probe.finish();
}
