//! Interval and certified surface (B30).
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
//! `Value::Interval` runtime variant landed. Today the runner answers
//! these fixtures with a symbolic pane (interval worlds are still a
//! gap), so the probe stays red until they run.

use emath_exec_ir::runner::run_package;
use emath_test_harness::{Probe, Source, boot};

fn demand_run_refusal(ph: &mut Probe, source: &Source, declaration: &str, needles: &[&str]) {
    let package = source.check().package;
    let report = run_package(&package);
    let Some(run) = report.declarations.iter().find(|run| run.name == declaration) else {
        ph.fail("declaration", format!("missing declaration {declaration}"));
        return;
    };
    let Some(test) = run.tests.iter().find(|test| test.name == "canonical").or_else(|| run.tests.first()) else {
        ph.fail("tests", format!("{declaration} ran no tests"));
        return;
    };
    if !test.verdict.is_refused() {
        ph.fail("verdict", format!("{declaration} must refuse at run, got {:?}", test.verdict));
        return;
    }
    let detail = format!("{:?}", test.verdict);
    for needle in needles {
        ph.contains(format!("detail:{needle}"), &detail, needle);
    }
}

#[test]
fn interval_surface_contracts() {
    boot();
    let mut ph = Probe::new(
        "certified intervals construct, propagate arithmetic bounds, and refuse zero-containing divisors or ill-formed bounds as run verdicts",
    );
    // The valid fixture admits and every `expect` passes: certified
    // construction and each arithmetic propagation row is evaluated by
    // the runner, never by a hand-written mirror.
    Source::from_workspace("tests/valid/interval_bounds.emath").eval_tests(&mut ph);

    let invalid = Source::from_workspace("tests/invalid/interval_bounds.emath");
    // Admission admits (the type layer sees element Float64); the
    // refusal is the run verdict — division by [-1, 1] must never
    // silently compute a widened interval.
    invalid.must_admit(&mut ph);
    ph.case("zero-containing-divisor-refuses-at-run", |ph| {
        demand_run_refusal(ph, &invalid, "SeededZeroContainingDivisor", &["contains zero"]);
    });
    ph.case("ill-formed-interval-refuses-at-construction", |ph| {
        demand_run_refusal(ph, &invalid, "SeededIllFormedInterval", &["lower bound exceeds upper"]);
    });
    // A scalar next to an interval never coerces: the mixed sum refuses
    // typed instead of silently widening the scalar.
    ph.case("mixed-interval-scalar-refuses-as-type-confusion", |ph| {
        let source = Source::from_str("mixed-interval-scalar", "package mixed_probe\n\nemath function M:\n    outputs:\n        bad: Interval<Float64>\n        probe: Float64\n    definitions:\n        bad = interval(1.0, 2.0) + 1.0\n        probe = 1.0\n    tests:\n        example <canonical>:\n            expect probe == 1\n");
        let package = source.check().package;
        let report = run_package(&package);
        let Some(run) = report.declarations.first() else {
            ph.fail("declaration", "no declaration ran");
            return;
        };
        let Some(test) = run.tests.iter().find(|test| test.name == "canonical").or_else(|| run.tests.first()) else {
            ph.fail("tests", "no test ran");
            return;
        };
        ph.demand(
            "refused",
            test.verdict.is_refused(),
            format!("mixed interval/scalar must refuse, got {:?}", test.verdict),
        );
    });
    ph.finish();
}
