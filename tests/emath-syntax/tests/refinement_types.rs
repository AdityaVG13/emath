//! Inline where-refinement leftovers refuse; ordinary functions still admit.

use emath_test_harness::{boot, Probe, Source};

fn check(text: &str, name: &str) -> Vec<String> {
    Source::from_str(name, text)
        .check()
        .diagnostics
        .errors()
        .map(|d| format!("{} {}", d.code, d.message))
        .collect()
}

const WHERE_REFINEMENT_ROW: &str = "\
emath function RefProbe:
    inputs:
        p: Float64 where 0 <= self and self <= 1

    outputs:
        f: Float64

    definitions:
        f = p * 2
";

const PLAIN: &str = "\
emath function PlainRefProbe:
    inputs:
        p: Float64

    outputs:
        f: Float64

    definitions:
        f = p * 2
";

#[test]
fn refinement_types() {
    boot();
    let mut probe = Probe::new("where-refinement leftovers refuse; ordinary functions admit");
    probe.case("inline_where_refinement_refuses", |p| {
        let errors = check(WHERE_REFINEMENT_ROW, "where-fence");
        p.demand(
            "1",
            !errors.is_empty(),
            format!("inline where-refinement must refuse, got {errors:#?}"),
        );
    });
    probe.case("plain_function_admits", |p| {
        let errors = check(PLAIN, "refine-plain-guard");
        p.demand(
            "1",
            errors.is_empty(),
            format!("ordinary function must still admit, got {errors:#?}"),
        );
    });
    probe.finish();
}
