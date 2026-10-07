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

/// An entry that exists but faults on a name inside its body (here an
/// object input `--set` cannot build) reports that fault, not "no
/// constructor entry". A truly unknown entry still says so.
#[test]
fn run_lane_reports_body_faults_not_a_missing_entry() {
    let mut p = Probe::new("run --function reports a fault inside the entry, not a missing entry");
    let src = std::env::temp_dir().join(format!("emath-run-entry-{}.emath", std::process::id()));
    std::fs::write(
        &src,
        "emath object Point:
    representation:
        x: Rat

emath function first_x:
    inputs:
        points: sequence(Point)
    outputs:
        result: Rat
    definitions:
        result = points[0].x

emath function forever:
    inputs:
        n: Int
    outputs:
        result: Int
    definitions:
        result = forever(n + 1)
",
    )
    .expect("write source");
    let path = src.to_string_lossy().to_string();
    let (text, code) = common::cli(&["run", &path, "--function", "first_x", "--set", "points=[{x: 1}]"]);
    p.eq("not a missing entry", text.contains("no constructor entry"), false);
    p.eq("fault exit", code, emath_cli::EXIT_FAULT as i32);
    let (text, code) = common::cli(&["run", &path, "--function", "missing", "--set", "n=1"]);
    p.contains("unknown entry", &text, "no constructor entry `missing`");
    p.eq("unknown entry exit", code, emath_cli::EXIT_ADMISSION as i32);
    // Running out of work is a partial answer, as on every other lane.
    let (text, code) = common::cli(&["run", &path, "--function", "forever", "--set", "n=0"]);
    p.contains("budget", &text, "budget_exhausted");
    p.eq("budget exit", code, emath_cli::EXIT_PARTIAL as i32);

    // A name the imported module does not export is an admission error,
    // whether `check` or `run --function` finds it.
    let dir = std::env::temp_dir().join(format!("emath-run-private-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    std::fs::write(
        dir.join("hidden.emath"),
        "emath function helper:\n    inputs:\n        n: Int\n    outputs:\n        result: Int\n    definitions:\n        result = n\n",
    )
    .expect("write module");
    let main = dir.join("main.emath");
    std::fs::write(
        &main,
        "use hidden\n\nemath function f:\n    inputs:\n        n: Int\n    outputs:\n        result: Int\n    definitions:\n        result = helper(n)\n",
    )
    .expect("write main");
    let (text, code) = common::cli(&["run", &main.to_string_lossy(), "--function", "f", "--set", "n=1"]);
    p.contains("private name", &text, "E-NAME-025");
    p.eq("private name exit", code, emath_cli::EXIT_ADMISSION as i32);
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

/// A run's `--worlds` list is part of its meaning: a run that suspends
/// and resumes through `emath step` must answer as the uninterrupted
/// run does, so `quote.worlds()` after the resume still names only the
/// worlds the run started with (not every world).
const WORLDS_AFTER_WORK: &str = "emath function WalkAt:
    inputs:
        n: Int
        i: Int
        acc: Int
    outputs:
        result: Int
    definitions:
        result = if i >= n: acc else: WalkAt(n, i + 1, acc + i)

emath function WorldsLater:
    inputs:
        n: Int
    outputs:
        result: sequence(Str)
    definitions:
        result = if WalkAt(n, 0, 0) >= 0: quote.worlds() else: quote.worlds()
";

/// An affine form held across a suspension keeps its noise symbols: after
/// `emath step`, `a - a` still cancels to zero and `a * a` is still an
/// affine form, not an interval or a point.
const AFFINE_ACROSS_STEP: &str = "emath function WalkAt:
    inputs:
        n: Int
        i: Int
        acc: Int
    outputs:
        result: Int
    definitions:
        result = if i >= n: acc else: WalkAt(n, i + 1, acc + i)

emath function Held:
    inputs:
        n: Int
    outputs:
        result: (Rat, Rat)
    definitions:
        keep = function y in Rat: y
        third = function y in Rat: y / 3
        t = quote.world(\"interval\", third, 1.0, 1000).value
        a = quote.world(\"affine\", keep, t, 1000).value
        result = if WalkAt(n, 0, 0) >= 0: (a - a, a * a) else: (a, a)
";

#[test]
fn step_resumes_a_held_affine_form() {
    let mut p = Probe::new("emath step keeps an affine form's noise symbols");
    let dir = std::env::temp_dir().join(format!("emath-step-affine-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let src = dir.join("affine_held.emath");
    std::fs::write(&src, AFFINE_ACROSS_STEP).expect("write source");
    let (text, code) = common::cli(&[
        "run",
        &src.to_string_lossy(),
        "--function",
        "Held",
        "--set",
        "n=500",
        "--work",
        "200",
        "--out",
        &dir.to_string_lossy(),
    ]);
    p.eq("suspends", code, EXIT_PARTIAL as i32);
    p.contains("suspended receipt", &text, "suspended");
    let checkpoint = dir.join("constructor-checkpoint.json");
    let (text, code) = common::cli(&["step", &checkpoint.to_string_lossy(), "--work", "2000000"]);
    p.eq("resume exit", code, EXIT_OK as i32);
    p.contains("a - a cancels", &text, "(0/1, ");
    p.contains("a * a stays affine", &text, " ± ");
    p.finish();
}

#[test]
fn step_resumes_under_the_runs_worlds() {
    let mut p = Probe::new("emath step keeps the suspended run's --worlds");
    let dir = std::env::temp_dir().join(format!("emath-step-worlds-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let src = dir.join("worlds_later.emath");
    std::fs::write(&src, WORLDS_AFTER_WORK).expect("write source");
    let (text, code) = common::cli(&[
        "run",
        &src.to_string_lossy(),
        "--function",
        "WorldsLater",
        "--set",
        "n=500",
        "--worlds",
        "exact,float64",
        "--work",
        "200",
        "--out",
        &dir.to_string_lossy(),
    ]);
    p.eq("suspends", code, EXIT_PARTIAL as i32);
    p.contains("suspended receipt", &text, "suspended");
    let checkpoint = dir.join("constructor-checkpoint.json");
    let (text, code) = common::cli(&["step", &checkpoint.to_string_lossy(), "--work", "2000000"]);
    p.eq("resume exit", code, EXIT_OK as i32);
    p.contains("first world", &text, "exact");
    p.contains("second world", &text, "float64");
    p.eq("no other world", text.contains("reference") || text.contains("interval"), false);
    p.finish();
}

#[test]
fn step_freezes_local_and_transitive_imports_across_repeated_resumes() {
    let dir = std::env::temp_dir().join(format!("emath-step-imports-{}-résearch", std::process::id()));
    std::fs::create_dir(&dir).expect("fresh scratch directory");
    let elsewhere = dir.join("elsewhere");
    std::fs::create_dir(&elsewhere).unwrap();
    let source = dir.join("main.emath");
    let helper = dir.join("helper.emath");
    let leaf = dir.join("leaf.emath");
    std::fs::write(
        &leaf,
        "# Unicode and codec-looking text stay source bytes: λ, source_len=7\nexports: Offset\n\nemath function Offset:\n    inputs:\n        n: Int\n    outputs:\n        result: Int\n    definitions:\n        result = n + 5\n",
    ).unwrap();
    std::fs::write(
        &helper,
        "exports: Finish\nuse leaf.{Offset as shifted}\n\nemath function Twice:\n    inputs:\n        x: Int\n    outputs:\n        result: Int\n    definitions:\n        result = x + x\n\nemath function Finish:\n    inputs:\n        x: Int\n    outputs:\n        result: Int\n    definitions:\n        result = Twice(x) + shifted(0)\n",
    ).unwrap();
    std::fs::write(
        &source,
        "use helper.{Finish as finish}\n\nemath function Walk:\n    inputs:\n        n: Int\n        i: Int\n        acc: Int\n    outputs:\n        result: Int\n    definitions:\n        result = if i >= n: acc else: Walk(n, i + 1, acc + i)\n\nemath function Main:\n    inputs:\n        n: Int\n    outputs:\n        result: Int\n    definitions:\n        result = Walk(n, 0, 0) + finish(1)\n",
    ).unwrap();
    let initial = std::process::Command::new(common::emath_bin())
        .args([
            "run", "main.emath", "--function", "Main", "--set", "n=200",
            "--work", "100", "--out", &dir.to_string_lossy(), "--json",
        ])
        .env("EMATH_ROOT", std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
        .current_dir(&dir)
        .output()
        .unwrap();
    assert_eq!(
        initial.status.code(), Some(EXIT_PARTIAL as i32),
        "{}{}", String::from_utf8_lossy(&initial.stdout), String::from_utf8_lossy(&initial.stderr)
    );
    let moved = elsewhere.join("saved-checkpoint.json");
    std::fs::copy(dir.join("constructor-checkpoint.json"), &moved).unwrap();
    // A path-only repair would either use these changed bytes or fail to load.
    for path in [&source, &helper, &leaf] {
        std::fs::write(path, "this changed file does not even parse\n").unwrap();
    }
    let step = |path: &std::path::Path, work: &str| {
        std::process::Command::new(common::emath_bin())
            .args(["step", &path.to_string_lossy(), "--work", work, "--out",
                &elsewhere.to_string_lossy(), "--json"])
            .current_dir(&elsewhere)
            .output()
            .expect("resume from a different directory")
    };
    let suspended = step(&moved, "100");
    assert_eq!(
        suspended.status.code(), Some(EXIT_PARTIAL as i32),
        "{}", String::from_utf8_lossy(&suspended.stdout)
    );
    let resumed = step(&elsewhere.join("constructor-checkpoint.json"), "50000");
    assert_eq!(
        resumed.status.code(), Some(EXIT_OK as i32),
        "{}{}", String::from_utf8_lossy(&resumed.stdout), String::from_utf8_lossy(&resumed.stderr)
    );
    assert!(
        String::from_utf8_lossy(&resumed.stdout).contains("\"payload\": \"19907\""),
        "{}", String::from_utf8_lossy(&resumed.stdout)
    );
}

#[test]
fn step_preserves_pending_scalar_literals() {
    let dir = std::env::temp_dir().join(format!("emath-step-rat-{}", std::process::id()));
    std::fs::create_dir(&dir).unwrap();
    let source = dir.join("main.emath");
    std::fs::write(
        &source,
        "emath function Walk:\n    inputs:\n        n: Int\n        acc: Int\n    outputs:\n        result: Int\n    definitions:\n        result = if n <= 0: acc else: Walk(n - 1, acc + n)\n\nemath function Main:\n    inputs:\n        n: Int\n    outputs:\n        result: Rat\n    definitions:\n        result = Walk(n, 0) / 1 + 7//13\n",
    ).unwrap();
    let (text, code) = common::cli(&[
        "run", &source.to_string_lossy(), "--function", "Main", "--set", "n=200",
        "--work", "50", "--out", &dir.to_string_lossy(),
    ]);
    assert_eq!(code, EXIT_PARTIAL as i32, "{text}");
    let (text, code) = common::cli(&[
        "step", &dir.join("constructor-checkpoint.json").to_string_lossy(), "--work", "50000",
    ]);
    assert_eq!(code, EXIT_OK as i32, "{text}");
    assert!(text.contains("261307/13"), "{text}");
    let message = dir.join("message.emath");
    std::fs::write(
        &message,
        "emath function Walk:\n    inputs:\n        n: Int\n    outputs:\n        result: Int\n    definitions:\n        result = if n <= 0: 1 else: Walk(n - 1)\n\nemath function Main:\n    inputs:\n        n: Int\n    outputs:\n        result: Str\n    definitions:\n        result = if Walk(n) > 0: \"kept λ\" else: \"wrong\"\n",
    ).unwrap();
    let (text, code) = common::cli(&[
        "run", &message.to_string_lossy(), "--function", "Main", "--set", "n=200",
        "--work", "50", "--out", &dir.to_string_lossy(),
    ]);
    assert_eq!(code, EXIT_PARTIAL as i32, "{text}");
    let (text, code) = common::cli(&[
        "step", &dir.join("constructor-checkpoint.json").to_string_lossy(), "--work", "50000",
    ]);
    assert_eq!(code, EXIT_OK as i32, "{text}");
    assert!(text.contains("kept λ"), "{text}");
}

#[test]
fn step_preserves_scoped_bindings_and_invariant_completion() {
    let dir = std::env::temp_dir().join(format!("emath-step-scopes-{}", std::process::id()));
    std::fs::create_dir(&dir).unwrap();
    let source = dir.join("main.emath");
    std::fs::write(
        &source,
        r#"emath function Walk:
    inputs:
        n: Int
    outputs:
        result: Int
    definitions:
        result = if n <= 0: 0 else: Walk(n - 1) + 1

emath object Checked:
    representation:
        n: Int
    invariants:
        first = Walk(n) == n
        second = n > 0

emath object Rejected:
    representation:
        n: Int
    invariants:
        first = Walk(n) == n
        second = false

emath object Secret:
    representation:
        kind = "abstract"

emath object Cells:
    representation:
        kind = "package"
    invariants:
        first = Walk(200) == 200

emath function Record:
    inputs:
        x: Int
    outputs:
        result: Int
    definitions:
        held = Checked: {n: 200}
        result = held.n + x

emath function Refusal:
    inputs:
        x: Int
    outputs:
        result: Int
    definitions:
        held = Rejected: {n: 200}
        result = x

emath function View:
    inputs:
        x: Int
    outputs:
        result: Int
    definitions:
        inside = quote.view x in quote(1): Walk(200)
        result = inside + x

emath function OpenQuote:
    inputs:
        x: Int
    outputs:
        result: Int
    definitions:
        packed = quote.view(quote(1 + 2)).children[0]
        inside = quote.open x in packed: Walk(200)
        result = inside + x

emath function OpenObject:
    inputs:
        x: Int
    outputs:
        result: Int
    definitions:
        packed = Secret.pack(17)
        inside = Secret.open x in packed: Walk(200) + x
        result = inside + x

emath function Packed:
    inputs:
        x: Int
    outputs:
        result: Int
    definitions:
        packed = Cells.pack(1, [x])
        result = packed.n + x

emath function Projection:
    inputs:
        x: Int
    outputs:
        result: Int
    definitions:
        result = (Checked: {n: Walk(200)}).n + x

emath function DelayedCode:
    inputs:
        n: Int
    outputs:
        result: Code
    definitions:
        result = if Walk(n) > 0: quote(200) else: quote(0)

emath function ViewDomain:
    inputs:
        x: Int
    outputs:
        result: Int
    definitions:
        inside = quote.view node in DelayedCode(200): node.value
        result = inside + x

emath function PackArgument:
    inputs:
        x: Int
    outputs:
        result: Int
    definitions:
        packed = Secret.pack(Walk(200))
        inside = Secret.open value in packed: value
        result = inside + x

emath function DelayedFunction:
    inputs:
        n: Int
    outputs:
        result: Int -> Int
    definitions:
        result = if Walk(n) > 0: function y in Int: y + 200 else: function y in Int: y

emath function Callee:
    inputs:
        x: Int
    outputs:
        result: Int
    definitions:
        result = DelayedFunction(200)(x)

emath function Choose:
    inputs:
        domain: Int
        action: Int -> Int
    outputs:
        result: Int
    definitions:
        result = action(domain)

emath function CallableBinder:
    inputs:
        x: Int
    outputs:
        result: Int
    definitions:
        inside = Choose y in Walk(200): y + 11
        result = inside + x
"#,
    )
    .unwrap();
    let mut failures = Vec::new();
    for (entry, payload, fault) in [
        ("Record", "211", false),
        ("Refusal", "object_invariant_failed", true),
        ("View", "211", false),
        ("OpenQuote", "211", false),
        ("OpenObject", "228", false),
        ("Packed", "12", false),
        ("Projection", "211", false),
        ("ViewDomain", "211", false),
        ("PackArgument", "211", false),
        ("Callee", "211", false),
        ("CallableBinder", "222", false),
    ] {
        let (direct, direct_exit) = common::cli(&[
            "run",
            &source.to_string_lossy(),
            "--function",
            entry,
            "--set",
            "x=11",
            "--work",
            "50000",
            "--json",
        ]);
        let expected_exit = if fault {
            emath_cli::EXIT_FAULT
        } else {
            EXIT_OK
        } as i32;
        assert_eq!(
            direct_exit, expected_exit,
            "{entry}: uninterrupted contract: {direct}"
        );
        for cut in ["50", "100", "200"] {
            let case = dir.join(format!("{entry}-{cut}"));
            std::fs::create_dir(&case).unwrap();
            let (text, code) = common::cli(&[
                "run",
                &source.to_string_lossy(),
                "--function",
                entry,
                "--set",
                "x=11",
                "--work",
                cut,
                "--out",
                &case.to_string_lossy(),
                "--json",
            ]);
            assert_eq!(code, EXIT_PARTIAL as i32, "{entry}/{cut}: {text}");
            // Stop once more inside the saved operation, not only at entry.
            let checkpoint = case.join("constructor-checkpoint.json");
            let (text, code) = common::cli(&[
                "step",
                &checkpoint.to_string_lossy(),
                "--work",
                "50",
                "--out",
                &case.to_string_lossy(),
                "--json",
            ]);
            assert_eq!(code, EXIT_PARTIAL as i32, "{entry}/{cut}: {text}");
            let (text, code) = common::cli(&[
                "step",
                &checkpoint.to_string_lossy(),
                "--work",
                "50000",
                "--json",
            ]);
            if code != expected_exit
                || !(text.contains(&format!("\"payload\": \"{payload}\""))
                    || fault && text.contains(payload))
            {
                failures.push(format!(
                    "{entry}/{cut}: exit {code}, expected {expected_exit}: {text}"
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
