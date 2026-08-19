//! Backend rendering and syntax-sanity tests.
//!
//! Moved from #[cfg(test)] in crates/emath-adapter-dew/src/backends.rs;
//! exercises the public backend API only.

use emath_adapter_dew::backends::{ident_sane, syntax_sane};
use emath_adapter_dew::dexpr::DewMatrix;
use emath_adapter_dew::{DewExpr, Layout, LinearOp, render_rust_fragment};

#[test]
fn oversized_integer_literal_is_refused() {
    let expr = DewExpr::Int(format!("400{}", "0".repeat(400)) + &"0".repeat(400));
    let err = render_rust_fragment(&expr).unwrap_err();
    assert!(err.contains("E-PROV-030"), "{err}");
}

#[test]
fn non_scalar_node_is_refused_never_stubbed_to_zero() {
    // A linear-algebra node under the scalar backend must be a typed
    // refusal: a `0.0` placeholder would smuggle a refused shape past
    // as computed output.
    let matrix = DewExpr::Matrix(DewMatrix {
        rows: 2,
        cols: 1,
        data: vec![DewExpr::Float64Bits(1.0f64.to_bits())],
        layout: Layout::RowMajor,
    });
    let linear = DewExpr::Linear(
        LinearOp::Scale,
        Box::new(matrix.clone()),
        Box::new(DewExpr::Float64Bits(2.0f64.to_bits())),
    );
    let err = render_rust_fragment(&linear).unwrap_err();
    assert!(err.contains("E-PROV-030"), "{err}");
    assert!(
        !err.contains("0.0"),
        "refusal must not carry a numeric placeholder: {err}"
    );
    let err = render_rust_fragment(&matrix).unwrap_err();
    assert!(err.contains("E-PROV-030"), "{err}");
}

#[test]
fn unsafe_identifier_is_refused() {
    let expr = DewExpr::Var("x-y".into());
    let err = render_rust_fragment(&expr).unwrap_err();
    assert!(err.contains("E-PROV-030"), "{err}");
}

#[test]
fn valid_expression_renders_fragment() {
    let expr = DewExpr::Var("temperature".into());
    let fragment = render_rust_fragment(&expr).expect("valid expression renders");
    assert!(
        fragment.text.contains("let t0: f64 = temperature;"),
        "{}",
        fragment.text
    );
    assert_eq!(fragment.anchors.len(), 1);
}

#[test]
fn syntax_sane_rejects_incomplete_statement() {
    assert!(syntax_sane("let x: f64 = 1.0;"));
    assert!(!syntax_sane("let {x: f64 = 1.0;"));
    assert!(!syntax_sane("(let x: f64 = 1.0"));
}

#[test]
fn ident_sane_rejects_hyphenated_name() {
    assert!(ident_sane("x"));
    assert!(ident_sane("_private"));
    assert!(!ident_sane("x-y"));
}
