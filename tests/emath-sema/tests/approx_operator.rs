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

use emath_exec_ir::interp::{Value, evaluate};
use emath_exec_ir::lower_definition;
use emath_test_harness::{Probe, Source, boot};

const TOLERANT: &str = "\
emath function A:
    inputs:
        x: Float64
    outputs:
        y: Float64
    definitions:
        y = x * x
    invariant:
        x ≈ (x * x + x) within rtol=1e-9, atol=0
";

const TOLERANT_ASCII: &str = "\
emath function A:
    inputs:
        x: Float64
    outputs:
        y: Float64
    definitions:
        y = x * x
    invariant:
        x ~= (x * x + x) within rtol=1e-9, atol=0
";

const BARE: &str = "\
emath function A:
    inputs:
        x: Float64
    outputs:
        y: Float64
    definitions:
        y = x * x
    invariant:
        x ≈ (x * x + x)
";

const IN_DEFINITIONS: &str = "\
emath function A:
    inputs:
        x: Float64
    outputs:
        y: Float64
    definitions:
        y = x ≈ (x * x) within rtol=1e-9, atol=0
";

const MACHINE_OUTSIDE: &str = "\
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

#[test]
fn approx_operator_honesty() {
    boot();
    let mut p = Probe::new("≈ admits with a declared tolerance, refuses bare, and is machine-checked");
    p.case("tolerance-admits", |p| {
        Source::from_str("tol-unicode", TOLERANT).must_admit(&mut *p);
        Source::from_str("tol-ascii", TOLERANT_ASCII).must_admit(&mut *p);
    });
    p.case("receipt", |p| {
        let admitted = Source::from_str("tol-receipt", TOLERANT).must_admit(&mut *p);
        let receipted = admitted.trace.entries.iter().any(|entry| {
            entry.detail.contains("≈") && entry.detail.contains("authority")
        });
        p.demand(
            "authority-degradation-recorded",
            receipted,
            format!(
                "expected an authority-degradation receipt for the ≈ edge, got {:?}",
                admitted
                    .trace
                    .entries
                    .iter()
                    .map(|entry| entry.detail.clone())
                    .collect::<Vec<_>>()
            ),
        );
    });
    p.case("bare-refuses", |p| {
        Source::from_str("bare", BARE).must_refuse(&mut *p, &["E-APPROX-TOL"]);
    });
    p.case("definitions-refuse", |p| {
        Source::from_str("in-definitions", IN_DEFINITIONS).must_refuse(&mut *p, &[]);
    });
    p.case("machine-check", |p| {
        Source::from_str("machine-outside", MACHINE_OUTSIDE).eval_tests(&mut *p);
        let outside = Source::from_str("machine-outside", MACHINE_OUTSIDE).check();
        match outside
            .package
            .declarations
            .first()
            .and_then(|decl| decl.invariants.first())
        {
            None => {
                p.fail(
                "machine-outside/claim",
                "admitted package carries no invariant claim to lower",
                );
            }
            Some(invariant) => {
                match lower_definition(&outside.package, *invariant, &["x".to_string()], &[]) {
                    Err(detail) => p.fail("machine-outside/lower", detail),
                    Ok(program) => p.eq(
                        "machine-outside/claim-false",
                        evaluate(&program, &[Value::F64(1.2)], &[]),
                        Ok(Value::Bool(false)),
                    ),
                };
            }
        }
        let inside_text = MACHINE_OUTSIDE.replace("1.2", "1.05");
        Source::from_str("machine-inside", &inside_text).eval_tests(&mut *p);
        let inside = Source::from_str("machine-inside", &inside_text).check();
        match inside
            .package
            .declarations
            .first()
            .and_then(|decl| decl.invariants.first())
        {
            None => {
                p.fail(
                "machine-inside/claim",
                "admitted package carries no invariant claim to lower",
                );
            }
            Some(invariant) => {
                match lower_definition(&inside.package, *invariant, &["x".to_string()], &[]) {
                    Err(detail) => p.fail("machine-inside/lower", detail),
                    Ok(program) => p.eq(
                        "machine-inside/claim-true",
                        evaluate(&program, &[Value::F64(1.05)], &[]),
                        Ok(Value::Bool(true)),
                    ),
                };
            }
        }
    });
    p.finish();
}
