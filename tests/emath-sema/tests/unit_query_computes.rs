//! Unit queries compute:
//! `unit of E` / `dimension of E` evaluate at admission as compile-time
//! comparisons against unit spellings, with typed receipts and refusals.
//!
//! Intent: the comparison `unit of E == <unit spelling>` derives E's
//! static unit from the type layer and compares dimension vectors; a
//! mismatching comparison is a typed refusal (E-UNIT-101), never a
//! silently-true claim. A bare `unit of E` used as a value stays a named
//! refuse (E-TYPE-010) — a unit is not a Phase-1 value.

use emath_test_harness::{boot, Probe, Source};

fn fn_with_x(extra: &str) -> String {
    format!(
        "emath function Q:\n    inputs:\n        x: Float64 in m\n        x2: Float64 in m\n    outputs:\n        y: Float64\n    definitions:\n        y = 2.0\n    constraints:\n{extra}"
    )
}

#[test]
fn unit_query_contract() {
    boot();
    let mut p = Probe::new("unit/dimension queries compute at admission");
    p.case("matching-spelling", |p| {
        Source::from_str("match", &fn_with_x("        unit of x == m\n")).must_admit(p);
    });
    p.case("mismatch", |p| {
        Source::from_str("mismatch", &fn_with_x("        unit of x == s\n"))
            .must_refuse(p, &["E-UNIT-101"]);
    });
    p.case("dimension", |p| {
        Source::from_str("dim-ok", &fn_with_x("        dimension of x == m\n")).must_admit(p);
        Source::from_str("dim-bad", &fn_with_x("        dimension of x == s\n"))
            .must_refuse(p, &["E-UNIT-101"]);
    });
    p.case("composed", |p| {
        Source::from_str("composed", &fn_with_x("        unit of (x * x) == m^2\n")).must_admit(p);
    });
    p.case("query-to-query", |p| {
        Source::from_str("qq-ok", &fn_with_x("        unit of x == unit of x2\n")).must_admit(p);
        Source::from_str(
            "qq-bad",
            "emath function Q:\n    inputs:\n        x: Float64 in m\n        t: Float64 in s\n    outputs:\n        y: Float64\n    definitions:\n        y = 2.0\n    constraints:\n        unit of x == unit of t\n",
        )
        .must_refuse(p, &["E-UNIT-101"]);
    });
    p.case("inequality", |p| {
        Source::from_str(
            "neq-true",
            "emath function Q:\n    inputs:\n        x: Float64 in m\n        t: Float64 in s\n    outputs:\n        y: Float64\n    definitions:\n        y = 2.0\n    constraints:\n        unit of x != unit of t\n",
        )
        .must_admit(p);
        Source::from_str("neq-false", &fn_with_x("        unit of x != m\n"))
            .must_refuse(p, &["E-UNIT-101"]);
    });
    p.case("bare-as-value", |p| {
        Source::from_str(
            "bare",
            "emath function Q:\n    inputs:\n        x: Float64 in m\n    outputs:\n        y: Float64\n    definitions:\n        y = unit of x\n",
        )
        .must_refuse(p, &["E-TYPE-010"]);
    });
    p.case("unknown-spelling", |p| {
        Source::from_str("unknown", &fn_with_x("        unit of x == Flurble\n"))
            .must_refuse(p, &["E-UNIT-104"]);
    });
    p.case("trace-receipt", |p| {
        let result = Source::from_str("receipt", &fn_with_x("        unit of x == m\n")).must_admit(p);
        let details: Vec<&str> = result.trace.entries.iter().map(|e| e.detail.as_str()).collect();
        p.demand(
            "receipt-recorded",
            details.iter().any(|d| d.contains("unit of")),
            format!("expected a `unit of` receipt on the trace, got {details:?}"),
        );
    });
    p.finish();
}
