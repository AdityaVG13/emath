//! Backend rendering and syntax-sanity tests.
use emath_adapter_dew::backends::{ident_sane, syntax_sane};
use emath_adapter_dew::dexpr::DewMatrix;
use emath_adapter_dew::{DewExpr, Layout, LinearOp, render_rust_fragment};
use emath_test_harness::{Case, Probe, check_all};

#[test]
fn probe() {
    let mut p = Probe::new("dew scalar backend renders valid fragments and refuses non-scalar/unsafe with E-PROV-030");
    let huge = DewExpr::Int(format!("400{}", "0".repeat(400)) + &"0".repeat(400));
    let matrix = DewExpr::Matrix(DewMatrix {
        rows: 2,
        cols: 1,
        data: vec![DewExpr::Float64Bits(1.0f64.to_bits())],
        layout: Layout::RowMajor,
    });
    let linear =
        DewExpr::Linear(LinearOp::Scale, Box::new(matrix.clone()), Box::new(DewExpr::Float64Bits(2.0f64.to_bits())));
    for (name, expr) in
        [("huge-int", &huge), ("matrix", &matrix), ("linear", &linear), ("hyphen-ident", &DewExpr::Var("x-y".into()))]
    {
        match render_rust_fragment(expr) {
            Ok(f) => {
                p.fail(name, format!("must refuse, rendered {f:?}"));
            }
            Err(e) => {
                p.contains(format!("{name}/code"), &e, "E-PROV-030");
                if name == "matrix" || name == "linear" {
                    p.demand(format!("{name}/no-placeholder"), !e.contains("0.0"), format!("refusal must not carry placeholder: {e}"));
                }
            }
        }
    }
    match render_rust_fragment(&DewExpr::Var("temperature".into())) {
        Err(e) => {
            p.fail("valid-render", format!("valid expression refused: {e}"));
        }
        Ok(f) => {
            p.contains("valid-render/text", &f.text, "let t0: f64 = temperature;");
            p.eq("valid-render/anchors", f.anchors.len(), 1);
        }
    }
    if let Err(m) = check_all(
        &[Case::new("complete", "let x: f64 = 1.0;", true), Case::new("brace", "let {x: f64 = 1.0;", false), Case::new("paren", "(let x: f64 = 1.0", false)],
        |s| syntax_sane(*s),
    ) {
        p.fail("syntax_sane", m);
    } else {
        p.demand("syntax_sane", true, "ok");
    }
    if let Err(m) = check_all(
        &[Case::new("x", "x", true), Case::new("private", "_private", true), Case::new("hyphen", "x-y", false)],
        |s| ident_sane(*s),
    ) {
        p.fail("ident_sane", m);
    } else {
        p.demand("ident_sane", true, "ok");
    }
    p.finish();
}
