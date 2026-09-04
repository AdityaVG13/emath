//! `≈` approximation labeling operator (04 section 6.4):
//! declared-tolerance admission, honest receipts, and the
//! typed refusal for tolerance-less approximations.
//!
//! Intent: approximation is the scientist's main verb and every language's
//! main lie. `≈` (ASCII `~=`) is a first-class relation that stamps
//! authority: a tolerance-carrying `≈` in a claim context admits as an
//! authority-degraded claim (receipt recorded, never silently exact); a
//! bare `≈` with no declared tolerance is refused (E-APPROX-TOL) — never
//! admitted as if it were exact, never silently dropped. Outside claim
//! contexts `≈` is not a computation and refuses.

use emath_core::limits::Limits;
use emath_exec_ir::interp::{Value, evaluate};
use emath_exec_ir::lower_definition;
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
        .check_owned("approx", source)
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

/// `≈` with a declared tolerance in a claim context admits, and the
/// receipt records the authority degradation (trace, not silence).
#[test]
fn approx_with_tolerance_admits_in_claim_context() {
    let out = check(&format!(
        "emath function A:\n    inputs:\n        x: Float64\n    outputs:\n        y: Float64\n    definitions:\n        y = x * x\n    invariant:\n        x ≈ (x * x + x) within rtol=1e-9, atol=0\n"
    ));
    assert!(
        !out.iter().any(|(severity, _)| severity == "Error"),
        "tolerance-carrying `≈` claim must admit, got {out:?}"
    );
}

/// ASCII spelling `~=` is the same operator and admits identically.
#[test]
fn approx_ascii_spelling_admits() {
    let out = check(&format!(
        "emath function A:\n    inputs:\n        x: Float64\n    outputs:\n        y: Float64\n    definitions:\n        y = x * x\n    invariant:\n        x ~= (x * x + x) within rtol=1e-9, atol=0\n"
    ));
    assert!(
        !out.iter().any(|(severity, _)| severity == "Error"),
        "`~=` must be the same operator as `≈`, got {out:?}"
    );
}

fn authority_source() -> String {
    format!(
        "emath function A:\n    inputs:\n        x: Float64\n    outputs:\n        y: Float64\n    definitions:\n        y = x * x\n    invariant:\n        x ≈ (x * x + x) within rtol=1e-9, atol=0\n"
    )
}

/// The authority-degradation receipt is recorded on the admitted claim
/// (never silently exact): the trace carries an approximation entry
/// naming the degraded authority.
#[test]
fn authority_degradation_receipt_is_recorded() {
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
    let result = session.check_owned("approx", &authority_source());
    assert!(
        !result.diagnostics.has_errors(),
        "claim must admit, got {:?}",
        result
            .diagnostics
            .errors()
            .map(|e| (e.code, e.message.clone()))
            .collect::<Vec<_>>()
    );
    let degraded = result
        .trace
        .entries
        .iter()
        .any(|entry| entry.detail.contains("≈") && entry.detail.contains("authority"));
    assert!(
        degraded,
        "expected an authority-degradation receipt for the ≈ edge, got {:?}",
        result
            .trace
            .entries
            .iter()
            .map(|entry| entry.detail.clone())
            .collect::<Vec<_>>()
    );
}

/// Negative control: bare `≈` with no declared tolerance is refused
/// (E-APPROX-TOL) — an approximation without a tolerance is the main lie,
/// never admitted as if exact.
#[test]
fn bare_approx_without_tolerance_refuses() {
    let out = check(&format!(
        "emath function A:\n    inputs:\n        x: Float64\n    outputs:\n        y: Float64\n    definitions:\n        y = x * x\n    invariant:\n        x ≈ (x * x + x)\n"
    ));
    assert!(
        out.iter()
            .any(|(severity, code)| severity == "Error" && code == "E-APPROX-TOL"),
        "bare `≈` must refuse E-APPROX-TOL, got {out:?}"
    );
}

/// Negative control: `≈` in a computation position is not a computation
/// and refuses (same honesty rule as `~~`).
#[test]
fn approx_in_definitions_is_not_a_computation() {
    let out = check(
        "emath function A:\n    inputs:\n        x: Float64\n    outputs:\n        y: Float64\n    definitions:\n        y = x ≈ (x * x) within rtol=1e-9, atol=0\n",
    );
    assert!(
        out.iter().any(|(severity, _)| severity == "Error"),
        "`≈` must refuse outside claim contexts, got {out:?}"
    );
}

#[test]
fn approximation_tolerance_is_machine_checked_at_run() {
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
    let source = "\
emath function NearOne:
    inputs:
        x: Float64
    outputs:
        y: Float64
    definitions:
        y = x
    invariant:
        x ≈ (x * 0.0 + 1.0) within rtol=0, atol=0.1
    tests:
        example <outside>:
            given x = 1.2
            expect y == 1.2
";
    let mut session = CompilerSession::new(Limits::default());
    let result = session.check_owned("approx-machine-check", source);
    assert!(
        !result.diagnostics.has_errors(),
        "{:?}",
        result.diagnostics.errors().collect::<Vec<_>>()
    );
    let declaration = &result.package.declarations[0];
    let program = lower_definition(
        &result.package,
        declaration.invariants[0],
        &["x".to_string()],
        &[],
    )
    .expect("approximation claim lowers");
    assert_eq!(
        evaluate(&program, &[Value::F64(1.2)], &[]),
        Ok(Value::Bool(false))
    );

    let inside = source.replace("1.2", "1.05");
    let mut session = CompilerSession::new(Limits::default());
    let result = session.check_owned("approx-machine-check-inside", &inside);
    assert!(!result.diagnostics.has_errors());
    let declaration = &result.package.declarations[0];
    let program = lower_definition(
        &result.package,
        declaration.invariants[0],
        &["x".to_string()],
        &[],
    )
    .expect("approximation claim lowers");
    assert_eq!(
        evaluate(&program, &[Value::F64(1.05)], &[]),
        Ok(Value::Bool(true))
    );
}
