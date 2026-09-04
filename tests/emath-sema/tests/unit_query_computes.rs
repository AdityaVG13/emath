//! Unit queries compute:
//! `unit of E` / `dimension of E` evaluate at admission as compile-time
//! comparisons against unit spellings, with typed receipts and refusals.
//!
//! Intent: the last parse-only CAPABILITY row becomes a computation. The
//! comparison `unit of E == <unit spelling>` derives E's static unit from
//! the type layer (`Infer::Unit` propagation through arithmetic) and
//! compares dimension vectors; a mismatching comparison is a typed
//! refusal (E-UNIT-101), never a silently-true claim. A bare `unit of E`
//! used as a value stays a named refuse (E-TYPE-010) — a unit is not a
//! Phase-1 value.

use emath_core::limits::Limits;
use emath_sema::CompilerSession;
use emath_syntax::install_source_parser;

fn check(source: &str) -> Vec<(String, String)> {
    {
        // Capsule admission resolves only through the installed language
        // distribution; install per thread before any session (rat_cells pattern).
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../language");
        let distribution = emath_exec_ir::language_image::load_language_distribution(&root)
            .expect("load capsule distribution");
        emath_sema::language::install_language_distribution(&distribution)
            .expect("install capsule-active kernels");
    }
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    session
        .check_owned("unit_query", source)
        .diagnostics
        .items()
        .iter()
        .map(|diagnostic| {
            (
                format!("{:?}", diagnostic.severity),
                diagnostic.code.to_string(),
            )
        })
        .collect()
}

fn fn_with_x(extra: &str) -> String {
    format!(
        "emath function Q:\n    inputs:\n        x: Float64 in m\n        x2: Float64 in m\n    outputs:\n        y: Float64\n    definitions:\n        y = 2.0\n    constraints:\n{extra}"
    )
}

/// `unit of x == m` in `constraints:` computes: the static unit of `x` (m) equals
/// the declared spelling (m) — admits with a receipt, never an error.
#[test]
fn unit_of_equality_against_matching_spelling_admits() {
    let out = check(&fn_with_x("        unit of x == m\n"));
    assert!(
        !out.iter().any(|(severity, _)| severity == "Error"),
        "matching unit equality must admit, got {out:?}"
    );
}

/// Mismatching unit comparison is a typed refusal (E-UNIT-101), never a
/// silently-true claim.
#[test]
fn unit_of_mismatch_refuses() {
    let out = check(&fn_with_x("        unit of x == s\n"));
    assert!(
        out.iter()
            .any(|(severity, code)| severity == "Error" && code == "E-UNIT-101"),
        "unit mismatch must refuse E-UNIT-101, got {out:?}"
    );
}

/// `dimension of E == <spelling>` compares the SI dimension vector.
#[test]
fn dimension_of_equality_computes() {
    let out = check(&fn_with_x("        dimension of x == m\n"));
    assert!(
        !out.iter().any(|(severity, _)| severity == "Error"),
        "matching dimension equality must admit, got {out:?}"
    );
    let out = check(&fn_with_x("        dimension of x == s\n"));
    assert!(
        out.iter()
            .any(|(severity, code)| severity == "Error" && code == "E-UNIT-101"),
        "dimension mismatch must refuse E-UNIT-101, got {out:?}"
    );
}

/// Unit composition through arithmetic: `unit of (x * x) == m^2` computes
/// through the Infer::Unit propagation (m·m = m²).
#[test]
fn unit_of_composed_expression_computes() {
    let out = check(&fn_with_x("        unit of (x * x) == m^2\n"));
    assert!(
        !out.iter().any(|(severity, _)| severity == "Error"),
        "unit of (x * x) == m^2 must admit, got {out:?}"
    );
}

/// Query-to-query comparison: two expressions with the same unit are
/// equal without naming a spelling.
#[test]
fn unit_of_to_unit_of_comparison_computes() {
    let out = check(&fn_with_x("        unit of x == unit of x2\n"));
    assert!(
        !out.iter().any(|(severity, _)| severity == "Error"),
        "same-unit comparison must admit, got {out:?}"
    );
    let out = check(
        "emath function Q:\n    inputs:\n        x: Float64 in m\n        t: Float64 in s\n    outputs:\n        y: Float64\n    definitions:\n        y = 2.0\n    constraints:\n        unit of x == unit of t\n",
    );
    assert!(
        out.iter()
            .any(|(severity, code)| severity == "Error" && code == "E-UNIT-101"),
        "m-unit vs s-unit must refuse E-UNIT-101, got {out:?}"
    );
}

/// Negation form: `!=` admits when the units differ, refuses when equal.
#[test]
fn unit_of_inequality_computes() {
    let out = check(
        "emath function Q:\n    inputs:\n        x: Float64 in m\n        t: Float64 in s\n    outputs:\n        y: Float64\n    definitions:\n        y = 2.0\n    constraints:\n        unit of x != unit of t\n",
    );
    assert!(
        !out.iter().any(|(severity, _)| severity == "Error"),
        "true inequality must admit, got {out:?}"
    );
    let out = check(&fn_with_x("        unit of x != m\n"));
    assert!(
        out.iter()
            .any(|(severity, code)| severity == "Error" && code == "E-UNIT-101"),
        "false inequality (m != m) must refuse E-UNIT-101, got {out:?}"
    );
}

/// Negative control (regression guard): a bare `unit of E` as a value
/// stays a named refuse — a unit is not a Phase-1 value.
#[test]
fn bare_unit_of_as_value_stays_refused() {
    let out = check(
        "emath function Q:\n    inputs:\n        x: Float64 in m\n    outputs:\n        y: Float64\n    definitions:\n        y = unit of x\n",
    );
    assert!(
        out.iter()
            .any(|(severity, code)| severity == "Error" && code == "E-TYPE-010"),
        "bare `unit of` as a value must refuse E-TYPE-010, got {out:?}"
    );
}

/// Unknown unit name in the comparison spelling is a typed refusal
/// (E-UNIT-104), never a silent admit.
#[test]
fn unknown_spelling_refuses() {
    let out = check(&fn_with_x("        unit of x == Flurble\n"));
    assert!(
        out.iter()
            .any(|(severity, code)| severity == "Error" && code == "E-UNIT-104"),
        "unresolvable dimension/unit name must refuse E-UNIT-104, got {out:?}"
    );
}

/// The receipt is recorded on the trace: the computed unit is data, never
/// a silent pass.
#[test]
fn unit_receipt_is_recorded() {
    {
        // Capsule admission resolves only through the installed language
        // distribution; install per thread before any session (rat_cells pattern).
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../language");
        let distribution = emath_exec_ir::language_image::load_language_distribution(&root)
            .expect("load capsule distribution");
        emath_sema::language::install_language_distribution(&distribution)
            .expect("install capsule-active kernels");
    }
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let result = session.check_owned("unit_query", &fn_with_x("        unit of x == m\n"));
    assert!(!result.diagnostics.has_errors());
    let receipt = result
        .trace
        .entries
        .iter()
        .any(|entry| entry.detail.contains("unit of"));
    assert!(
        receipt,
        "expected a `unit of` receipt on the trace, got {:?}",
        result
            .trace
            .entries
            .iter()
            .map(|entry| entry.detail.clone())
            .collect::<Vec<_>>()
    );
}
