//! edition tests migrated from the in-crate `#[cfg(test)]` module.

use emath_core::{DeprecationStage, Edition};
use emath_syntax::edition::*;

#[test]
fn edition_2026_selects_grammar_2026_1() {
    let profile = grammar_profile_for("2026").expect("2026 ships");
    assert_eq!(profile.grammar_version, "2026.1");
    assert_eq!(profile.edition, Edition::Ed2026);
}

#[test]
fn unknown_edition_is_typed_refusal() {
    let Err(error) = grammar_profile_for("2099") else {
        assert!(false, "2099 must not ship");
        unreachable!();
    };
    assert_eq!(error.code, emath_core::E_PKG_EDITION_UNKNOWN);
}

#[test]
fn recognized_admitted_hidden_not() {
    assert!(admitted_by_default(DeprecationStage::Recognized, "2026"));
    assert!(!admitted_by_default(DeprecationStage::Deprecated, "2026"));
    assert!(!admitted_by_default(DeprecationStage::Hidden, "2026"));
    // Frozen is replay-only: never admitted by a default table.
    assert!(!admitted_by_default(DeprecationStage::Frozen, "2026"));
}

#[test]
fn grammar_table_covers_every_shipped_edition() {
    for edition in Edition::ALL {
        assert!(
            grammar_profile_for(edition.as_str()).is_ok(),
            "{} unshipped",
            edition
        );
    }
}
