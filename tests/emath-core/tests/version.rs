//! version tests migrated from the in-crate `#[cfg(test)]` module:
//! every symbol they exercise is public crate surface.

use emath_core::version::*;

#[test]
fn version_constants_are_nonempty_and_stable() {
    assert!(!EMATH_REFERENCE_VERSION.is_empty());
    assert!(!EMATH_GRAMMAR_VERSION.is_empty());
    assert!(!EMATH_CANON_ENCODING_VERSION.is_empty());
    assert_eq!(VERSION_STACK.len(), 4);
    assert_eq!(VERSION_STACK[0], ("reference", EMATH_REFERENCE_VERSION));
}

#[test]
fn edition_2026_resolves() {
    assert_eq!(Edition::from_manifest_str("2026"), Ok(Edition::Ed2026));
    assert_eq!(Edition::Ed2026.grammar_version(), "2026.1");
}

#[test]
fn unknown_edition_is_typed_refusal() {
    let error = Edition::from_manifest_str("2099").expect_err("2099 not shipped");
    assert_eq!(error.code, E_PKG_EDITION_UNKNOWN);
    assert_eq!(error.value, "2099");
    assert!(error.to_string().contains("shipped editions: 2026, 2030"));
}

#[test]
fn deprecation_ladder_order_is_total() {
    assert_eq!(DeprecationStage::ALL.len(), 4);
    assert!(DeprecationStage::Recognized < DeprecationStage::Deprecated);
    assert!(DeprecationStage::Deprecated < DeprecationStage::Hidden);
    assert!(DeprecationStage::Hidden < DeprecationStage::Frozen);
    assert_eq!(DeprecationStage::Frozen.as_str(), "frozen");
}
