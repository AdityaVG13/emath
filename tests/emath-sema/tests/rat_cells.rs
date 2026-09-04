//! Failure-first tests for the Rat exact-rational capability cells.
//!
//! Contract: Rat = exact rational, canonical num/den (i128), den > 0,
//! gcd-reduced at every step. The cells are capsule-active
//! (`std.capability.exact.rat*`), so every probe must admit, evaluate
//! exactly through the installed Language Image, and refuse typed.

use std::collections::BTreeMap;

use emath_core::limits::Limits;
use emath_exec_ir::interp::{EvalFault, Value};
use emath_exec_ir::language_image::load_language_distribution;
use emath_exec_ir::runner::{TestVerdict, eval_definitions_values};
use emath_sema::language::install_language_distribution;
use emath_sema::{CheckResult, CompilerSession};

/// Admit one `.emath` source and return the full checked result.
fn check(source: &str) -> CheckResult {
    // Capsule surface admission and kernel binding both resolve through
    // the installed language distribution; every test runs on its own
    // thread, so each check installs it for that thread.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../language");
    let distribution = load_language_distribution(&root).expect("load capsule distribution");
    install_language_distribution(&distribution).expect("install capsule-active kernels");
    emath_syntax::install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    // `check_owned` returns the result directly; parse/lex failures land
    // in `diagnostics` and surface where each test asserts on them.
    session.check_owned("rat-cells.emath", source)
}

/// Evaluate the first declaration of an admitted package over `bindings`.
fn eval_first(
    source: &str,
    bindings: &[(&str, Value)],
) -> std::collections::BTreeMap<String, Value> {
    let result = check(source);
    let given: BTreeMap<String, Value> = bindings
        .iter()
        .map(|(name, value)| (name.to_string(), value.clone()))
        .collect();
    eval_definitions_values(
        &result.package,
        &result.package.declarations[0],
        &given,
        &BTreeMap::new(),
    )
    .unwrap_or_else(|fault| panic!("rat cell must evaluate: {fault}"))
}

/// rat_add(1/3, 1/6) == 1/2 EXACT — canonical num/den, not f64.
#[test]
fn rat_add_exact_halves() {
    let values = eval_first(
        "emath function probe:\n    inputs:\n        n: Int\n    outputs:\n        c: Rat\n    definitions:\n        c = rat_add(rat(1, n), rat(1, 6))\n",
        &[("n", Value::I64(3))],
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
        "emath function probe:\n    inputs:\n        n: Int\n    outputs:\n        c: Rat\n    definitions:\n        c = rat_norm(rat(n, 4))\n",
        &[("n", Value::I64(6))],
    );
    assert_eq!(values.get("c"), Some(&Value::Rat { num: 3, den: 2 }));
}

/// A denominator that would lose precision as f64 (1/(10^18+7)) stays
/// EXACT through the cell surface — the pass-6 no-hidden-float seed.
#[test]
fn rat_large_denominator_stays_exact() {
    let values = eval_first(
        "emath function probe:\n    inputs:\n        n: Int\n    outputs:\n        c: Rat\n    definitions:\n        c = rat(1, n)\n",
        &[("n", Value::I64(1_000_000_000_000_000_007))],
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
        "emath function probe:\n    inputs:\n        n: Int\n    outputs:\n        c: Rat\n    definitions:\n        c = rat(1, n)\n",
    );
    assert!(
        result.diagnostics.errors().count() == 0,
        "admission must accept the capsule surface: {:?}",
        result.diagnostics.errors().collect::<Vec<_>>()
    );
    let verdict = eval_definitions_values(
        &result.package,
        &result.package.declarations[0],
        &[("n".to_string(), Value::I64(0))].into_iter().collect(),
        &BTreeMap::new(),
    )
    .err()
    .expect("rat(1, 0) must refuse");
    let TestVerdict::Fault {
        fault: EvalFault::CapabilityRefused { code, .. },
    } = verdict
    else {
        panic!("expected a typed capability refusal, got {verdict:?}");
    };
    assert!(
        code.contains("denominator"),
        "refusal must name the denominator, got: {code}"
    );
}
