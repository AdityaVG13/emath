//! `grad(...)` refuses: the Wengert-tape builtin was retired by the
//! constructor constitution (emath-xx0x.1) and differentiation is
//! authored (`analysis.autodiff` / `analysis.derivative`, the exact
//! Rat tier). The admission lane must refuse the spelling by name
//! instead of lowering it onto the retired native kernels.
mod common;

use std::path::PathBuf;

#[test]
fn grad_refuses_by_name() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/constructor/grad_retired.emath");
    let (text, code) = common::cli(&["check", fixture.to_str().expect("utf8 fixture path")]);
    assert!(
        code != 0,
        "grad must refuse admission (retired builtin); check admitted it:\n{text}"
    );
    assert!(
        text.contains("E-TYPE-010"),
        "the refusal must name its diagnostic code; output:\n{text}"
    );
    assert!(
        text.contains("grad"),
        "the refusal must name the retired spelling; output:\n{text}"
    );
}
