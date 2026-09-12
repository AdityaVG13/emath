//! Unit catalogs and `constraints:` are not constructor surface.

use emath_test_harness::{boot, Probe, Source};

fn fn_with_x(extra: &str) -> String {
    format!(
        "emath function Q:\n    inputs:\n        x: Float64 in m\n        x2: Float64 in m\n    outputs:\n        y: Float64\n    definitions:\n        y = 2.0\n    constraints:\n{extra}"
    )
}

#[test]
fn unit_query_contract() {
    boot();
    let mut p = Probe::new("unit annotations and constraints: refuse");
    p.case("unit-field-gone", |p| {
        Source::from_str("match", &fn_with_x("        unit of x == m\n"))
            .must_refuse(p, &["E-KIND-GONE"]);
    });
    p.case("constraints-gone", |p| {
        Source::from_str(
            "plain",
            "emath function Q:\n    inputs:\n        x: Float64\n    outputs:\n        y: Float64\n    definitions:\n        y = x\n    constraints:\n        x >= 0\n",
        )
        .must_refuse(p, &["E-SEC-101"]);
    });
    p.finish();
}
