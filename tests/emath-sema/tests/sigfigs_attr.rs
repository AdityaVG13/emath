//! `@significant_figures` admission (
//! 04 §1.6): display/enforce modes, precision warning receipts, and the
//! typed refusals for malformed specs.
//!
//! Intent: sig-figs are a display contract, not uncertainty propagation.
//! Enforce mode turns under-reported literals into warning receipts (never
//! refusals); malformed specs and unknown modes are typed refusals
//! (E-SYN-117), never silent drops.

use emath_test_harness::{boot, Probe, Source};

fn function_source(prefix: &str, definitions: &str) -> String {
    format!(
        "{prefix}emath function P:\n    inputs:\n        x: Float64\n    outputs:\n        y: Float64\n    definitions:\n{definitions}    goals:\n        evaluate <y>\n"
    )
}

#[test]
fn sigfigs_contract() {
    boot();
    let mut p = Probe::new("sigfigs display/enforce admission contract");
    p.case("display-silent", |p| {
        let src = function_source("@significant_figures(display)\n", "        y = x * 1.5\n");
        let result = Source::from_str("display", &src).must_admit(p);
        let all: Vec<String> = result
            .diagnostics
            .items()
            .iter()
            .map(|d| format!("{:?}:{}", d.severity, d.code))
            .collect();
        p.demand("no-diagnostics", all.is_empty(), format!("expected silence, got {all:?}"));
    });
    p.case("display-with-count", |p| {
        let src = function_source("@significant_figures(display, 4)\n", "        y = x * 1.500\n");
        let result = Source::from_str("display-count", &src).must_admit(p);
        let all: Vec<String> = result
            .diagnostics
            .items()
            .iter()
            .map(|d| format!("{:?}:{}", d.severity, d.code))
            .collect();
        p.demand("no-diagnostics", all.is_empty(), format!("expected silence, got {all:?}"));
    });
    p.case("enforce-warn-receipt", |p| {
        let src = function_source("@significant_figures(enforce, 3)\n", "        y = x * 1.5\n");
        let result = Source::from_str("enforce-warn", &src).check();
        let items: Vec<(String, String)> = result
            .diagnostics
            .items()
            .iter()
            .map(|d| (format!("{:?}", d.severity), d.code.to_string()))
            .collect();
        p.demand(
            "under-report-warns",
            items.iter().any(|(s, c)| s == "Warning" && c == "E-SF-UNDER-REPORT"),
            format!("expected E-SF-UNDER-REPORT warning receipt, got {items:?}"),
        );
        p.demand(
            "warn-never-refuses",
            items.iter().all(|(s, _)| s != "Error"),
            format!("warning receipts never refuse, got {items:?}"),
        );
    });
    p.case("enforce-compliant-silent", |p| {
        let src = function_source("@significant_figures(enforce, 3)\n", "        y = x * 1.50\n");
        let result = Source::from_str("enforce-ok", &src).must_admit(p);
        let all: Vec<String> = result
            .diagnostics
            .items()
            .iter()
            .map(|d| format!("{:?}:{}", d.severity, d.code))
            .collect();
        p.demand("no-diagnostics", all.is_empty(), format!("expected silence, got {all:?}"));
    });
    p.case("enforce-without-count", |p| {
        let src = function_source("@significant_figures(enforce)\n", "        y = x * 1.5\n");
        Source::from_str("enforce-bare", &src).must_refuse(p, &["E-SYN-117"]);
    });
    p.case("unknown-mode", |p| {
        let src = function_source("@significant_figures(precision)\n", "        y = x * 1.5\n");
        Source::from_str("unknown-mode", &src).must_refuse(p, &["E-SYN-117"]);
    });
    p.case("mixed-kinds", |p| {
        let src = function_source("@significant_figures(display)\n", "        y = x * (1.5 ± 0.02) + 2.0\n");
        let result = Source::from_str("mixed", &src).check();
        let items: Vec<(String, String)> = result
            .diagnostics
            .items()
            .iter()
            .map(|d| (format!("{:?}", d.severity), d.code.to_string()))
            .collect();
        p.demand(
            "mixed-warns",
            items.iter().any(|(s, c)| s == "Warning" && c == "E-SF-MIXED-KINDS"),
            format!("expected E-SF-MIXED-KINDS warning receipt, got {items:?}"),
        );
        p.demand(
            "warn-never-refuses",
            items.iter().all(|(s, _)| s != "Error"),
            format!("mixing kinds warns, never refuses, got {items:?}"),
        );
    });
    p.finish();
}
