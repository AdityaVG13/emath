//! `@significant_figures` admission (bead emath-r3-sigfigs-formatting-yf28,
//! 04 §1.6): display/enforce modes, precision warning receipts, and the
//! typed refusals for malformed specs.
//!
//! Intent: sig-figs are a display contract, not uncertainty propagation.
//! Enforce mode turns under-reported literals into warning receipts (never
//! refusals); malformed specs and unknown modes are typed refusals
//! (E-SYN-117), never silent drops.

use emath_core::limits::Limits;
use emath_sema::CompilerSession;
use emath_syntax::install_source_parser;

/// (severity, code) pairs so tests can assert warning receipts without
/// confusing them with refusals.
fn check(source: &str) -> Vec<(String, String)> {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    session
        .check_owned("sigfigs", source)
        .diagnostics
        .items()
        .iter()
        .map(|diagnostic| (format!("{:?}", diagnostic.severity), diagnostic.code.to_string()))
        .collect()
}

fn function_source(prefix: &str, definitions: &str) -> String {
    format!(
        "{prefix}emath function P:\n    inputs:\n        x: Float64\n    outputs:\n        y: Float64\n    definitions:\n{definitions}    goals:\n        evaluate <y>\n"
    )
}

/// Positive control: `display` mode admits with no diagnostic at all.
#[test]
fn display_mode_admits_silently() {
    let out = check(&function_source(
        "@significant_figures(display)\n",
        "        y = x * 1.5\n",
    ));
    assert!(
        !out.iter().any(|(_, code)| code == "E-SYN-118"),
        "display mode must be a known attribute, got {out:?}"
    );
    assert!(out.is_empty(), "expected no diagnostics, got {out:?}");
}

/// `display` accepts an optional sf count: `@significant_figures(display, 4)`.
#[test]
fn display_mode_with_count_admits() {
    let out = check(&function_source(
        "@significant_figures(display, 4)\n",
        "        y = x * 1.500\n",
    ));
    assert!(out.is_empty(), "expected no diagnostics, got {out:?}");
}

/// Enforce mode: a literal with fewer sf than declared is a WARNING
/// receipt, and the file still admits (no errors).
#[test]
fn enforce_mode_under_report_is_a_warning_receipt() {
    let out = check(&function_source(
        "@significant_figures(enforce, 3)\n",
        "        y = x * 1.5\n",
    ));
    assert!(
        out.iter()
            .any(|(severity, code)| severity == "Warning" && code == "E-SF-UNDER-REPORT"),
        "expected an E-SF-UNDER-REPORT warning receipt, got {out:?}"
    );
    assert!(
        !out.iter().any(|(severity, _)| severity == "Error"),
        "warning receipts never refuse, got {out:?}"
    );
}

/// Enforce mode: a literal meeting the declared sf count stays silent.
#[test]
fn enforce_mode_compliant_literal_is_silent() {
    let out = check(&function_source(
        "@significant_figures(enforce, 3)\n",
        "        y = x * 1.50\n",
    ));
    assert!(out.is_empty(), "expected no diagnostics, got {out:?}");
}

/// Negative control: `enforce` without an sf count is a typed refusal
/// (the threshold must be explicit), never a silent default.
#[test]
fn enforce_without_count_refuses() {
    let out = check(&function_source(
        "@significant_figures(enforce)\n",
        "        y = x * 1.5\n",
    ));
    assert!(
        out.iter()
            .any(|(severity, code)| severity == "Error" && code == "E-SYN-117"),
        "expected E-SYN-117, got {out:?}"
    );
}

/// Negative control: an unknown mode is a typed refusal, never silent.
#[test]
fn unknown_mode_refuses() {
    let out = check(&function_source(
        "@significant_figures(precision)\n",
        "        y = x * 1.5\n",
    ));
    assert!(
        out.iter()
            .any(|(severity, code)| severity == "Error" && code == "E-SYN-117"),
        "expected E-SYN-117, got {out:?}"
    );
}

/// Negative control: mixing Measured (uncertainty) values with bare
/// sf-values under one precision contract is a warning receipt, never a
/// refusal and never silent (bead test plan).
#[test]
fn mixing_measured_with_bare_sf_warns() {
    let out = check(&function_source(
        "@significant_figures(display)\n",
        "        y = x * (1.5 ± 0.02) + 2.0\n",
    ));
    assert!(
        out.iter()
            .any(|(severity, code)| severity == "Warning" && code == "E-SF-MIXED-KINDS"),
        "expected an E-SF-MIXED-KINDS warning receipt, got {out:?}"
    );
    assert!(
        !out.iter().any(|(severity, _)| severity == "Error"),
        "mixing kinds warns, never refuses, got {out:?}"
    );
}
