//! `emath run --json` receipts carry work-consumed telemetry (bead
//! emath-7zplf): tuning `--work` needs the measured consumption, not
//! just the raised limit. Failure-first: the envelope carried no work
//! datum at all before the fix.

mod common;
use emath_cli::{EXIT_OK, EXIT_PARTIAL};
use emath_test_harness::Probe;

const TRI: &str = "emath function Tri:
    inputs:
        n: Int
    outputs:
        result: Int
    definitions:
        result = n * (n + 1)
";

#[test]
fn probe() {
    let mut p = Probe::new("run receipts report consumed work units");
    let src = std::env::temp_dir().join(format!("emath-work-receipt-{}.emath", std::process::id()));
    std::fs::write(&src, TRI).expect("write source");
    let (text, code) = common::cli(&[
        "run",
        &src.to_string_lossy(),
        "--function",
        "Tri",
        "--set",
        "n=1000",
        "--work",
        "2000000",
        "--json",
    ]);
    p.eq("exit", code, EXIT_OK as i32);
    p.contains("work telemetry", &text, "\"work_consumed\"");
    p.contains("payload", &text, "1001000");
    p.finish();
}

/// A module whose authored test suite needs more than the constructor
/// layer's DEFAULT_WORK gate must still be runnable through
/// `--function`: the run lane evaluates one entry, not the module's
/// tests, and `--work` is the budget that applies. Before the gate fix
/// the run lane evaluated the whole test tree at the DEFAULT_WORK
/// budget BEFORE reading `--work`, so any module with a suite above
/// that default refused `budget_exhausted` at every budget. The heavy
/// work lives in the GIVEN: given-stage budget exhaustion is a hard
/// tree error, so the pre-fix gate refuses on this fixture. Failure-
/// first evidence: the same command on the pre-fix gate exits
/// EXIT_PARTIAL with `budget_exhausted` and no payload.
const SUITE_OVER_DEFAULT: &str = "emath function WalkAt:
    inputs:
        n: Int
        i: Int
        acc: Int
    outputs:
        result: Int
    definitions:
        result = if i >= n: acc else: WalkAt(n, i + 1, acc + i)

emath function Walk:
    inputs:
        n: Int
    outputs:
        result: Int
    definitions:
        result = WalkAt(n, 0, 0)
    tests:
        example <suite_above_default_work>:
            given n = Walk(200000)
            expect n == 19999900000

emath function Cheap:
    inputs:
        unused: Int
    outputs:
        result: Rat
    definitions:
        result = 1 / 1
";

/// The module-lane counterpart: the same walk pinned by a literal
/// given, so the suite is a legitimate pass under a raised `--work`
/// (the walk costs ~2.2M work units, above the DEFAULT_WORK gate,
/// below `--work 5000000`). Without `--function` the run lane DOES
/// evaluate the suite, and `--work` is the budget that evaluation runs
/// under. Before the fix the tree was evaluated at DEFAULT_WORK with
/// `--work` unread, so this module exited EXIT_PARTIAL (suite test
/// failed at the default) even with a generous `--work`.
/// Failure-first evidence: on the pre-fix gate the `--work 5000000`
/// command exits EXIT_PARTIAL instead of EXIT_OK.
const SUITE_HEAVY_UNDER_WORK: &str = "emath function WalkAt:
    inputs:
        n: Int
        i: Int
        acc: Int
    outputs:
        result: Int
    definitions:
        result = if i >= n: acc else: WalkAt(n, i + 1, acc + i)

emath function Walk:
    inputs:
        n: Int
    outputs:
        result: Int
    definitions:
        result = WalkAt(n, 0, 0)
    tests:
        example <suite_under_raised_work>:
            given n = 200000
            expect result == 19999900000
";

#[test]
fn run_lane_skips_module_suite_for_function_runs() {
    let mut p = Probe::new("run --function skips the module suite; --work applies to the entry");
    let src = std::env::temp_dir().join(format!(
        "emath-run-gate-{}.emath",
        std::process::id()
    ));
    std::fs::write(&src, SUITE_OVER_DEFAULT).expect("write source");
    let (text, code) = common::cli(&[
        "run",
        &src.to_string_lossy(),
        "--function",
        "Cheap",
        "--set",
        "unused=0",
        "--work",
        "2000000",
    ]);
    p.eq("exit", code, EXIT_OK as i32);
    p.contains("cheap payload", &text, "1/1");
    p.eq("no budget refusal", text.contains("budget_exhausted"), false);
    p.finish();
}

/// The other side of the same hunk: without `--function` the run lane
/// DOES evaluate the module suite, and `--work` is the budget that
/// evaluation runs under. Two demands pin it: a generous `--work` runs
/// the suite green (exit OK), and the default budget still refuses it
/// (exit PARTIAL), proving the suite is really being evaluated and
/// that `--work`, not the default, is the budget in force.
#[test]
fn run_lane_honors_work_for_module_runs() {
    let mut p = Probe::new("run without --function evaluates the suite under --work");
    let src = std::env::temp_dir().join(format!(
        "emath-run-gate-mod-{}.emath",
        std::process::id()
    ));
    std::fs::write(&src, SUITE_HEAVY_UNDER_WORK).expect("write source");
    let (_, code) = common::cli(&["run", &src.to_string_lossy(), "--work", "5000000"]);
    p.eq("raised work exits ok", code, EXIT_OK as i32);
    let (_, code) = common::cli(&["run", &src.to_string_lossy()]);
    p.eq("default budget still partial", code, EXIT_PARTIAL as i32);
    p.finish();
}
