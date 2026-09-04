//!: interval and certified surface (B30).
//!
//! - `interval(a, b)` constructs a certified interval (C18: no `∎`
//!   glyph; function-style constructor).
//! - Interval arithmetic propagates certified bounds: `[a,b] + [c,d] =
//!   [a+c, b+d]`, subtraction mirrors, multiplication encloses all four
//!   corner products, division multiplies by the flipped reciprocal of a
//!   zero-free divisor.
//! - A zero-CONTAINING divisor is a typed run refusal, never a silently
//!   widened interval. An ill-formed interval (`lo > hi`, non-finite
//!   bound) refuses at construction.
//! - Admission admits `Interval<Float64>` as its element type (existing
//!   doctrine): the refusals below are RUN verdicts from the interp
//!   world, not admission errors.
//!
//! Failure-first: every pin below was RED until the `IntervalCreate` /
//! `IntervalIntersect` ops, the `interval`/`intersect` builtins, and the
//! `Value::Interval` runtime variant landed.

use emath_exec_ir::interp::Value;
use emath_exec_ir::runner::run_package;
use emath_sema::CompilerSession;
use emath_syntax::install_source_parser;

const VALID: &str = include_str!("../../../tests/valid/interval_bounds.emath");
const INVALID: &str = include_str!("../../../tests/invalid/interval_bounds.emath");

fn check(source: &str) -> Vec<(String, String)> {
    install_source_parser();
    let mut session = CompilerSession::new(emath_core::limits::Limits::default());
    session
        .check_owned("r3_intervals_8pjn", source)
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

fn error_codes(source: &str) -> Vec<String> {
    check(source)
        .into_iter()
        .filter(|(severity, _)| severity == "Error")
        .map(|(_, code)| code)
        .collect()
}

fn interval_of(value: &Value) -> Option<(f64, f64)> {
    match value {
        Value::Interval { lo, hi } => Some((*lo, *hi)),
        _ => None,
    }
}

fn check_owned_package(source: &str) -> emath_ir::SemanticPackage {
    install_source_parser();
    let mut session = CompilerSession::new(emath_core::limits::Limits::default());
    session.check_owned("r3_intervals_8pjn", source).package
}

mod r3_intervals_8pjn {
    use super::*;

    #[test]
    fn interval_constructor_builds_a_certified_interval() {
        let errors = error_codes(VALID);
        assert!(errors.is_empty(), "interval surface must admit: {errors:?}");
        let report = run_package(&check_owned_package(VALID));
        let run = &report.declarations[0];
        let x = run.tests[0].outputs.get("x").expect("x computed");
        assert_eq!(
            interval_of(x),
            Some((1.0, 2.0)),
            "interval(1.0, 2.0) must construct the certified bounds"
        );
    }

    #[test]
    fn interval_arithmetic_propagates_certified_bounds() {
        let report = run_package(&check_owned_package(VALID));
        let run = report
            .declarations
            .iter()
            .find(|declaration| declaration.name == "IntervalArithmetic")
            .expect("arithmetic declaration runs");
        let cases: [(&str, f64, f64); 5] = [
            ("interval_sum", 4.0, 6.0),
            ("interval_difference", -3.0, -1.0),
            ("interval_product", -10.0, 15.0),
            ("interval_quotient", 1.5, 4.0),
            ("interval_overlap", 4.0, 5.0),
        ];
        for (name, lo, hi) in cases {
            let value = run.tests[0].outputs.get(name).expect(name);
            assert_eq!(
                interval_of(value),
                Some((lo, hi)),
                "{name} must propagate certified bounds"
            );
        }
    }

    #[test]
    fn zero_containing_divisor_refuses_at_run() {
        // Admission admits (the type layer sees element Float64); the
        // refusal is the run verdict — division by [-1, 1] must never
        // silently compute a widened interval.
        let errors = error_codes(INVALID);
        assert!(
            errors.is_empty(),
            "admission admits; refusal is at run: {errors:?}"
        );
        let report = run_package(&check_owned_package(INVALID));
        let run = report
            .declarations
            .iter()
            .find(|declaration| declaration.name == "SeededZeroContainingDivisor")
            .expect("declaration runs");
        assert!(
            run.tests[0].verdict.is_refused(),
            "zero-containing divisor must refuse, got {:?}",
            run.tests[0].verdict
        );
        let detail = format!("{:?}", run.tests[0].verdict);
        assert!(
            detail.contains("contains zero"),
            "refusal must name the zero-containing divisor: {detail}"
        );
    }

    #[test]
    fn ill_formed_interval_refuses_at_construction() {
        let report = run_package(&check_owned_package(INVALID));
        let run = report
            .declarations
            .iter()
            .find(|declaration| declaration.name == "SeededIllFormedInterval")
            .expect("declaration runs");
        assert!(
            run.tests[0].verdict.is_refused(),
            "interval(2.0, 1.0) must refuse, got {:?}",
            run.tests[0].verdict
        );
        let detail = format!("{:?}", run.tests[0].verdict);
        assert!(
            detail.contains("lower bound exceeds upper"),
            "refusal must name the ill-formed bounds: {detail}"
        );
    }

    #[test]
    fn mixed_interval_scalar_operands_refuse_as_type_confusion() {
        // A scalar next to an interval never coerces: the mixed sum
        // refuses typed instead of silently widening the scalar.
        let source = "package mixed_probe\n\nemath function M:\n    outputs:\n        bad: Interval<Float64>\n        probe: Float64\n    definitions:\n        bad = interval(1.0, 2.0) + 1.0\n        probe = 1.0\n    tests:\n        example <canonical>:\n            expect probe == 1\n";
        let report = run_package(&check_owned_package(source));
        let run = &report.declarations[0];
        assert!(
            run.tests[0].verdict.is_refused(),
            "mixed interval/scalar must refuse, got {:?}",
            run.tests[0].verdict
        );
    }
}
