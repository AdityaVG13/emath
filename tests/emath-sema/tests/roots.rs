//! Root-solving robustness (Track A3, passes 5-6).
//! The `solve(f) wrt x` surface lowers to `EmirOp::Solve`: Newton with
//! a deterministic bracket-discovery + bisection fallback whenever the
//! gradient is unreliable (vanished derivative or non-finite value).
//! These tests drive the FALLBACK through the full language pipeline
//! (parse → admit → emitter → interpreter) with failing-capable
//! fixtures: a flat seed where Newton cannot step, a bracketless
//! residual, and a pole (sign flips across a discontinuity). The
//! fallback must find the root deterministically or refuse with the
//! typed fault — never hang, never invent a root.

use emath_exec_ir::interp::{EvalFault, Value};
use emath_exec_ir::runner::{RunReport, run_package};
use emath_test_harness::{Probe, Source, boot};

const FLAT_SEED_SQRT2: &str = "\
emath function FlatSeedSqrt2:
    inputs:
        x: Float64

    outputs:
        r: Float64

    definitions:
        f = x * x - 2
        r = solve(f) wrt x

    tests:
        example <flat>:
            given x = 0
            expect abs(r * r - 2) < 1e-6
            expect r > 0.5
";

const FLAT_SEED_CUBIC: &str = "\
emath function FlatSeedCubic:
    inputs:
        x: Float64

    outputs:
        r: Float64

    definitions:
        f = x * x * x - 8
        r = solve(f) wrt x

    tests:
        example <flat>:
            given x = 0
            expect abs(r - 2) < 1e-6
";

const BRACKETLESS_RESIDUAL: &str = "\
emath function BracketlessResidual:
    inputs:
        x: Float64

    outputs:
        r: Float64

    definitions:
        f = x * x + 1
        r = solve(f) wrt x

    tests:
        example <none>:
            given x = 0
            expect r == 0.0
";

const POLE_RESIDUAL: &str = "\
emath function PoleResidual:
    inputs:
        x: Float64

    outputs:
        r: Float64

    definitions:
        f = 1.0 / x
        r = solve(f) wrt x

    tests:
        example <none>:
            given x = 0
            expect r == 0.0
";

const ROOT_SCALING_INVARIANCE: &str = "\
emath function RootScalingInvariance:
    inputs:
        x: Float64

    outputs:
        r1: Float64
        r2: Float64

    definitions:
        f = x * x - 2
        r1 = solve(f) wrt x
        r2 = solve(7.0 * f) wrt x

    tests:
        example <invariance>:
            given x = 0
            expect abs(r1 - r2) < 1e-9
            expect abs(r1 * r1 - 2) < 1e-6
";

const ROOT_SEED_INVARIANCE: &str = "\
emath function RootSeedInvariance:
    inputs:
        x: Float64

    outputs:
        r: Float64

    definitions:
        f = x * x - 2
        r = solve(f) wrt x

    tests:
        example <seed0>:
            given x = 0
            expect abs(r * r - 2) < 1e-6
        example <seed5>:
            given x = 5
            expect abs(r * r - 2) < 1e-6
";

fn arithmetic_detail(verdict: &emath_exec_ir::runner::TestVerdict) -> Option<&str> {
    match verdict {
        emath_exec_ir::runner::TestVerdict::Fault {
            fault: EvalFault::Arithmetic { detail, .. },
        } => Some(detail),
        _ => None,
    }
}

fn run_admitted(p: &mut Probe, name: &str, source: &str) -> RunReport {
    let result = Source::from_str(name, source).must_admit(p);
    run_package(&result.package)
}

fn scalar_output(p: &mut Probe, case: &str, report: &RunReport, key: &str) -> f64 {
    match report.declarations[0].tests[0].outputs.get(key) {
        Some(Value::F64(value)) => *value,
        other => {
            p.fail(format!("{case}:output"), format!("{key} must be a scalar, got {other:?}"));
            f64::NAN
        }
    }
}

#[test]
fn solve_falls_back_deterministically_or_refuses_typed() {
    boot();
    let mut p = Probe::new("solve(f) wrt x finds the root deterministically or refuses with the typed fault");
    // In-language oracles: every solvable residual carries `tests:` with real
    // numeric expects (sqrt(2)^2 = 2, cbrt(8) = 2); eval_tests demands Passed.
    Source::from_str("flat-sqrt2", FLAT_SEED_SQRT2).eval_tests(&mut p);
    Source::from_str("flat-cubic", FLAT_SEED_CUBIC).eval_tests(&mut p);
    Source::from_str("scaling", ROOT_SCALING_INVARIANCE).eval_tests(&mut p);
    Source::from_str("seed", ROOT_SEED_INVARIANCE).eval_tests(&mut p);
    p.case("flat-sqrt2", |p| {
        // df = 2x vanishes at the seed, so Newton cannot step; the bracket
        // scan + bisection must find sqrt(2), byte-identically every run.
        let first = run_admitted(p, "flat-sqrt2-a", FLAT_SEED_SQRT2);
        let second = run_admitted(p, "flat-sqrt2-b", FLAT_SEED_SQRT2);
        p.eq("deterministic", &first, &second);
        let test = &first.declarations[0].tests[0];
        p.demand("passed", test.verdict.expect_passed(), format!("must pass: {}", test.verdict));
        let root = scalar_output(p, "flat-sqrt2", &first, "r");
        p.close("value", root, std::f64::consts::SQRT_2, 1e-6);
        p.close("residual", root * root, 2.0, 1e-6);
        p.demand("positive", root > 0.5, format!("root ~= +sqrt(2), got {root}"));
    });
    p.case("flat-cubic", |p| {
        // df = 3x^2 vanishes at the seed; the single real root is x = 2.
        let report = run_admitted(p, "flat-cubic-run", FLAT_SEED_CUBIC);
        let test = &report.declarations[0].tests[0];
        p.demand("passed", test.verdict.expect_passed(), format!("must pass: {}", test.verdict));
        let root = scalar_output(p, "flat-cubic", &report, "r");
        p.close("value", root, 2.0, 1e-6);
    });
    p.case("bracketless", |p| {
        // x^2 + 1 has no real root: vanished derivative and no sign change
        // must refuse with the typed fault, never hang or invent a root.
        let report = run_admitted(p, "bracketless-run", BRACKETLESS_RESIDUAL);
        let test = &report.declarations[0].tests[0];
        p.demand("refused", test.verdict.is_refused(), format!("must refuse, got {}", test.verdict));
        p.eq(
            "fault",
            arithmetic_detail(&test.verdict),
            Some("solve derivative vanished before convergence"),
        );
    });
    p.case("pole", |p| {
        // 1/x at the seed 0 is non-finite and the sign change is a pole,
        // not a root: the refusal must name the nonfinite fault.
        let report = run_admitted(p, "pole-run", POLE_RESIDUAL);
        let test = &report.declarations[0].tests[0];
        p.demand("refused", test.verdict.is_refused(), format!("must refuse, got {}", test.verdict));
        p.eq(
            "fault",
            arithmetic_detail(&test.verdict),
            Some("solve produced a nonfinite value and found no sign-changing bracket in the deterministic scan"),
        );
    });
    p.case("scaling", |p| {
        // Multiplying by nonzero 7 preserves the zero set: solve(7*f) must
        // land on the same root, and the residual must vanish there.
        let report = run_admitted(p, "scaling-run", ROOT_SCALING_INVARIANCE);
        let r1 = scalar_output(p, "scaling", &report, "r1");
        p.close("residual", r1.powi(2), 2.0, 1e-6);
    });
    p.case("seed", |p| {
        // Seed 0 (flat derivative, fallback-driven) and seed 5 (Newton)
        // must both land on a root of x^2 - 2.
        let report = run_admitted(p, "seed-run", ROOT_SEED_INVARIANCE);
        p.eq("examples", report.declarations[0].tests.len(), 2);
        for test in &report.declarations[0].tests {
            p.demand(
                format!("{}:passed", test.name),
                test.verdict.expect_passed(),
                format!("must pass: {}", test.verdict),
            );
        }
    });
    p.finish();
}
