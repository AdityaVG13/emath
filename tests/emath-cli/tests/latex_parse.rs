//! Tests for latex.rs, migrated out of production code.
use emath_cli_lab::layout::LayoutError;
use emath_cli_lab::layout::{parse_latex, to_binder_term};
use emath_genesis::{BinderBudget, BinderDomain, BinderFamily, BinderKind, BinderTerm};
use emath_term::SymbolId;
use emath_test_harness::{Case, Probe, check_all, expect_ok};
#[test]
fn probe() {
    let mut p = Probe::new("latex lowers sums structurally, refusals name offsets");
    p.case("source", |p| { let g = parse_latex(r"\sum_{i=1}^{3} i").expect("parse"); p.eq("bytes", g.source(), r"\sum_{i=1}^{3} i"); });
    p.case("sum", |p| {
        let t = to_binder_term(&parse_latex(r"\sum_{i=1}^{3} i").expect("parse")).expect("lower");
        let BinderTerm::Bind(b) = t else { p.fail("bind", "sum must lower to Bind"); return; };
        p.eq("kind", b.kind.clone(), BinderKind::Sum); p.eq("family", b.family.clone(), BinderFamily::Structural); p.eq("domain", b.domain.clone(), BinderDomain::FiniteRange { lower: 1, upper: 3 });
        p.eq("expand", b.expand(&SymbolId("+".to_string()), BinderBudget::default()).expect("expand").canonical(), "apply(+,apply(+,const(1),const(2)),const(3))".to_string());
    });
    expect_ok(check_all(&[Case::new("macro", r"x+\foo", LayoutError::UnknownMacro { name: "foo".to_string(), offset: 2 }), Case::new("dollar", "hello $foo", LayoutError::UnterminatedDollar { offset: 6 })], |s| parse_latex(s).expect_err("refuses")));
    p.case("region", |p| { let s = r"see $\sum_{i=1}^{3} i$ please"; let g = parse_latex(s).expect("parse"); let (a, b) = g.formula_regions().next().expect("region").source_span; p.eq("span", &s[a..b], r"$\sum_{i=1}^{3} i$"); });
    p.finish();
}
