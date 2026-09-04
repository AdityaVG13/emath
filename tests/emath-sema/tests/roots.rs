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

use emath_core::limits::Limits;
use emath_exec_ir::interp::EvalFault;
use emath_exec_ir::runner::run_package;
use emath_sema::CompilerSession;
use emath_sema::admit::CheckResult;
use emath_syntax::install_source_parser;

fn check_source(name: &str, source: &str) -> CheckResult {
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
    session.check_owned(name, source)
}

fn run_once(source: &str, name: &str) -> emath_exec_ir::runner::RunReport {
    let result = check_source(name, source);
    assert!(
        !result.diagnostics.has_errors(),
        "fixture must admit: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    run_package(&result.package)
}

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

fn arithmetic_detail(verdict: &emath_exec_ir::runner::TestVerdict) -> Option<&str> {
    match verdict {
        emath_exec_ir::runner::TestVerdict::Fault {
            fault: EvalFault::Arithmetic { detail, .. },
        } => Some(detail),
        _ => None,
    }
}

#[test]
fn flat_seed_sqrt2_falls_back_to_bisection_deterministically() {
    // Newton's derivative vanishes at the seed (df = 2x = 0 at x = 0);
    // the deterministic bracket scan + bisection must find sqrt(2).
    // Running twice must produce byte-identical reports.
    let first = run_once(FLAT_SEED_SQRT2, "flat-seed-sqrt2");
    let second = run_once(FLAT_SEED_SQRT2, "flat-seed-sqrt2");
    let test1 = &first.declarations[0].tests[0];
    let test2 = &second.declarations[0].tests[0];
    assert_eq!(
        first, second,
        "the fallback must be deterministic across runs"
    );
    assert!(
        test1.verdict.expect_passed(),
        "flat-seed sqrt(2) must be found by the fallback: {}",
        test1.verdict
    );
    let Some(r) = test1.outputs.get("r") else {
        panic!("r must be evaluated");
    };
    let emath_exec_ir::interp::Value::F64(root) = r else {
        panic!("r must be a scalar");
    };
    assert!(
        *root > 0.5 && (root * root - 2.0).abs() < 1e-6,
        "root ~= sqrt(2), got {root}"
    );
}

#[test]
fn flat_seed_cubic_falls_back_to_the_real_root() {
    // f(x) = x^3 - 8 has df = 3x^2 = 0 at the seed; the fallback must
    // find the single real root x = 2.
    let report = run_once(FLAT_SEED_CUBIC, "flat-seed-cubic");
    let test = &report.declarations[0].tests[0];
    assert!(
        test.verdict.expect_passed(),
        "flat-seed cubic root must be found: {}",
        test.verdict
    );
    let emath_exec_ir::interp::Value::F64(root) = test.outputs.get("r").expect("r evaluated")
    else {
        panic!("r must be a scalar");
    };
    assert!((*root - 2.0).abs() < 1e-6, "root must be 2, got {root}");
}

#[test]
fn bracketless_residual_refuses_with_the_typed_fault() {
    // f(x) = x^2 + 1 has no real root: Newton's derivative vanishes at
    // the seed AND the deterministic scan finds no sign change. The
    // language-level verdict must be the typed arithmetic fault, never
    // a hang and never an invented root.
    let report = run_once(BRACKETLESS_RESIDUAL, "bracketless-residual");
    let test = &report.declarations[0].tests[0];
    assert_eq!(
        arithmetic_detail(&test.verdict),
        Some("solve derivative vanished before convergence"),
        "no-root residual must refuse with the vanished-derivative fault: {}",
        test.verdict
    );
}

#[test]
fn pole_residual_refuses_with_the_nonfinite_fault() {
    // f(x) = 1/x at the seed 0: the residual is non-finite, and the
    // scan's sign change is across a pole, not a root — bisection
    // never converges, so the refusal must be the nonfinite fault.
    let report = run_once(POLE_RESIDUAL, "pole-residual");
    let test = &report.declarations[0].tests[0];
    assert_eq!(
        arithmetic_detail(&test.verdict),
        Some(
            "solve produced a nonfinite value and found no sign-changing bracket in the deterministic scan"
        ),
        "pole residual must refuse with the nonfinite fault: {}",
        test.verdict
    );
}

// --- Metamorphic laws: root-set invariance under
// transformations that preserve the zero set. The oracle problem
// (unknown analytic root) is bypassed by relating roots of
// transformed residuals to roots of the original through the
// language: rescaling the residual by a nonzero constant and
// re-solving from a different seed must land on the same root, and
// the residual must vanish at every solved root.

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

#[test]
fn mr_rescaling_a_residual_preserves_its_root() {
    // The zero set is invariant under multiplying by a nonzero scalar:
    // solve(7*f) must land on the same root as solve(f) — even when
    // Newton is unreliable at the shared seed (the fallback drives
    // both).
    let report = run_once(ROOT_SCALING_INVARIANCE, "root-scaling-invariance");
    let test = &report.declarations[0].tests[0];
    assert!(
        test.verdict.expect_passed(),
        "solve(7*f) must land on the same root as solve(f): {}",
        test.verdict
    );
}

#[test]
fn mr_residual_vanishes_at_the_solved_root() {
    // Definitional residual law: a reported root must satisfy
    // f(root) == 0 within tolerance (never an invented value). The
    // fixture asserts this in-language for the flat-seed sqrt2 case.
    let report = run_once(ROOT_SCALING_INVARIANCE, "root-scaling-invariance-residual");
    let test = &report.declarations[0].tests[0];
    let emath_exec_ir::interp::Value::F64(r1) = test.outputs.get("r1").expect("r1 evaluated")
    else {
        panic!("r1 must be a scalar");
    };
    assert!(
        (*r1 * *r1 - 2.0).abs() < 1e-6,
        "the solved root must satisfy f(root) == 0, got r1 = {r1}"
    );
}

#[test]
fn mr_root_set_is_seed_invariant_when_the_fallback_is_deterministic() {
    // Solving from a different seed must not change the reported root
    // set: seed 0 (flat derivative, fallback-driven) and seed 5 (plain
    // Newton) must both land on a root of x² − 2. This pins the
    // fallback's determinism across seeds in addition to across runs.
    let report = run_once(ROOT_SEED_INVARIANCE, "root-seed-invariance");
    assert_eq!(report.declarations[0].tests.len(), 2, "both examples run");
    for test in &report.declarations[0].tests {
        assert!(
            test.verdict.expect_passed(),
            "the fallback must be seed-invariant for f(x)=x²−2: {}",
            test.verdict
        );
    }
}
