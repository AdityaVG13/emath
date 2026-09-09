//! One-line solve/plot scratch desugars to admitted meaning.
use emath_syntax::{expand_scratch, parse_str};
use emath_test_harness::Probe;

#[test]
fn scratch_desugars_to_admitted_meaning() {
    let mut p = Probe::new("one-line solve/plot scratch admits identically to its expansion");
    for source in ["solve x^2 = 2 over Real\n", "plot sin(x) on -3.14..3.14\n"] {
        p.case(source.trim(), |p| {
            let expansion = expand_scratch(source);
            p.demand("rewritten", expansion.rewritten(), "scratch must rewrite");
            p.demand("scratch-diag", !expansion.diagnostics.has_errors(), format!("{:?}", expansion.diagnostics));
            let (direct, direct_diags) = parse_str(source);
            let (expanded, expanded_diags) = parse_str(&expansion.expanded);
            p.demand("direct-admits", !direct_diags.has_errors(), format!("{direct_diags:?}"));
            p.demand("expanded-admits", !expanded_diags.has_errors(), expansion.expanded.clone());
            p.eq("identical", direct, expanded);
        });
    }
    p.finish();
}
