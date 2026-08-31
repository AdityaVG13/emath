//! sigfigs tests migrated from the in-crate `#[cfg(test)]` module:
//! every symbol they exercise is public crate surface.

use emath_core::sigfigs::*;

#[test]
fn sf_convention_holds() {
    assert_eq!(count_sig_figs("1230"), Some(3));
    assert_eq!(count_sig_figs("1.230"), Some(4));
    assert_eq!(count_sig_figs("0.0012"), Some(2));
    assert_eq!(count_sig_figs("1000."), Some(4));
    assert_eq!(count_sig_figs("abc"), None);
    assert_eq!(count_sig_figs("0.0"), None);
}

#[test]
fn enforce_ladder() {
    let spec = SigFigSpec {
        mode: SigFigMode::Enforce,
        count: 3,
    };
    assert!(spec.enforce_check(2).is_some());
    assert!(spec.enforce_check(3).is_none());
    let display = SigFigSpec {
        mode: SigFigMode::Display,
        count: 3,
    };
    assert!(
        display.enforce_check(1).is_none(),
        "display mode never warns"
    );
}
