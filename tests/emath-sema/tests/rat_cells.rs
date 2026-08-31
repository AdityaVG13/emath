//! Failure-first tests for the Rat exact-rational capability cells
//! (bead emath-rat-real-types-p5cj, pass 3 prep).
//!
//! Contract: Rat = exact rational, canonical num/den (i128), den > 0,
//! gcd-reduced at every step. Every cell below is MISSING on the current
//! tree — each test must FAIL now (unknown-name refusal) and pass once
//! the cells register.

use emath_core::limits::Limits;
use emath_exec_ir::interp::Value;
use emath_sema::{CheckResult, CompilerSession};

/// Admit one `.emath` source and return the full checked result.
fn check(source: &str) -> CheckResult {
    emath_syntax::install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    session
        .check_owned("rat-cells.emath", source)
        .expect("source must parse and admit")
}

/// Evaluate the first declaration of an admitted package over no bindings.
fn eval_first(source: &str) -> std::collections::BTreeMap<String, Value> {
    let result = check(source);
    emath_exec_ir::runner::eval_definitions_values(
        &result.package,
        &result.package.declarations[0],
        &std::collections::BTreeMap::new(),
        &std::collections::BTreeMap::new(),
    )
    .unwrap_or_else(|fault| panic!("rat cell must evaluate: {fault}"))
}

/// rat_add(1/3, 1/6) == 1/2 EXACT — canonical num/den, not f64.
#[test]
fn rat_add_exact_halves() {
    let values = eval_first(
        "emath function probe:\n    outputs:\n        c: Rat\n    definitions:\n        c = rat_add(rat(1, 3), rat(1, 6))\n",
    );
    assert_eq!(
        values.get("c"),
        Some(&Value::Rat { num: 1, den: 2 }),
        "rat_add(1/3, 1/6) must be the exact canonical 1/2"
    );
}

/// rat_norm(6/4) == 3/2 — gcd-reduced, den forced positive.
#[test]
fn rat_norm_gcd_reduces_and_canonicalizes_sign() {
    let values = eval_first(
        "emath function probe:\n    outputs:\n        c: Rat\n    definitions:\n        c = rat_norm(rat(6, 4))\n",
    );
    assert_eq!(values.get("c"), Some(&Value::Rat { num: 3, den: 2 }));
}

/// A denominator that would lose precision as f64 (1/(10^18+7)) stays
/// EXACT through the cell surface — the pass-6 no-hidden-float seed.
#[test]
fn rat_large_denominator_stays_exact() {
    let values = eval_first(
        "emath function probe:\n    outputs:\n        c: Rat\n    definitions:\n        c = rat(1, 1000000000000000007)\n",
    );
    assert_eq!(
        values.get("c"),
        Some(&Value::Rat {
            num: 1,
            den: 1_000_000_000_000_000_007
        })
    );
}

/// Zero denominator is a TYPED refusal, never a panic and never a silent 0.
#[test]
fn rat_zero_denominator_refused() {
    let result = check(
        "emath function probe:\n    outputs:\n        c: Rat\n    definitions:\n        c = rat(1, 0)\n",
    );
    assert!(
        result
            .diagnostics
            .errors()
            .any(|e| e.to_string().contains("denominator")),
        "rat(1, 0) must refuse with a typed denominator diagnostic, got: {:?}",
        result.diagnostics.errors().collect::<Vec<_>>()
    );
}
