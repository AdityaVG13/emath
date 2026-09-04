//! Failure-first harness for `emath sweep` (bead emath-6gz8m).
//!
//! Contract under test: `emath sweep <file.emath> --function F --grid
//! p=17,41 z=9,27 [--expect name=value] [--out result.json] [--json]`
//! runs the cartesian parameter grid over ONE admitted `emath function`
//! through the SAME generic stack as `emath eval` (sema admission, EMIR
//! lowering, reference VM), prints one deterministic line per cell, and
//! emits a deterministic JSON artifact `emath.sweep.v1` (meaning_id +
//! grid + per-cell results, no wall-clock). Exit codes: 0 every cell ok,
//! 1 any expectation mismatch or evaluation error, 2 usage.
//!
//! Determinism contract: cells enumerate in axis order with the FIRST
//! axis slowest and each axis's values in CLI order; bindings render in
//! grid-axis order, outputs in declaration order; the artifact carries
//! `meaning_id` and never a wall-clock field.
//!
//! Failure-first: this file ran RED against the pre-sweep binary
//! (`unknown command sweep`, exit 2) and only turned green after the
//! sweep lane landed. The acceptance test replays the 15 rows of
//! `internal/proximity-prize/sweep-results.txt` byte-exactly.

use std::path::PathBuf;
use std::process::Command;

mod common;

use emath_artifact::{JsonValue, parse_json_document};
use emath_cli::{CliExit, EXIT_OK, EXIT_REFUSED, EXIT_USAGE};

fn fixture_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("emath-cli-sweep-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn write_fixture(dir: &PathBuf, name: &str, source: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, source).expect("write fixture");
    path
}

fn powerword() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../internal/proximity-prize/powerword-zero-sum.emath")
}

/// Spawn the real prebuilt binary for `sweep <file> [...]` (never
/// `cargo run` per invocation, mirroring the sibling suite): parallel
/// cargo invocations interleave build chatter with stdout and would
/// splice the artifact frames.
fn sweep(path: &str, args: &[&str]) -> (String, String, i32) {
    let output = Command::new(common::emath_bin())
        .arg("sweep")
        .arg(path)
        .args(args)
        .output()
        .expect("run emath binary");
    (
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
        output.status.code().unwrap_or(-1),
    )
}

fn expect_exit(code: i32, wanted: CliExit) {
    assert_eq!(code, wanted as i32, "exit code");
}

fn parse_artifact(stdout: &str) -> JsonValue {
    match parse_json_document(stdout) {
        Ok(parsed) => parsed,
        Err(error) => panic!("stdout must be the sweep artifact: {error}\n{stdout}"),
    }
}

/// Rendered cell bindings in artifact order.
fn cell_pairs(cell: &JsonValue, field: &str) -> Vec<(String, String)> {
    match cell.field(field).expect(field) {
        JsonValue::Obj(entries) => entries
            .iter()
            .map(|(name, value)| match value {
                JsonValue::Str(text) => (name.clone(), text.clone()),
                other => panic!("{field} {name} must be a rendered string, got {other:?}"),
            })
            .collect(),
        other => panic!("{field} must be an object, got {other:?}"),
    }
}

fn cell_status(cell: &JsonValue) -> String {
    cell.string_field("status").expect("cell status")
}

const SQ1: &str = "\
emath function Sq:
    inputs:
        p: Int
    outputs:
        y: Int
    definitions:
        y = p * p
";

const ADD2: &str = "\
emath function Add2:
    inputs:
        p: Int
        z: Int
    outputs:
        s: Int
    definitions:
        s = p + z
";

const DUAL: &str = "\
emath function Dual:
    inputs:
        p: Int
        z: Int
    outputs:
        s: Int
        m: Int
    definitions:
        s = p + z
        m = p * z
";

/// The grid parser admits `name=v1,v2,...` axis specs, keeps CLI axis
/// order, and expands the cartesian product with the first axis
/// slowest, each axis's values in CLI order.
#[test]
fn sweep_grid_parser_cartesian_order() {
    let dir = fixture_dir("cartesian");
    let path = write_fixture(&dir, "add2.emath", ADD2);
    let path = path.to_string_lossy().into_owned();
    let (stdout, stderr, code) = sweep(
        &path,
        &["--function", "Add2", "--grid", "p=2,3", "z=10,20", "--json"],
    );
    expect_exit(code, EXIT_OK);
    assert!(
        stderr.is_empty(),
        "success must keep stderr clean: {stderr}"
    );
    let doc = parse_artifact(&stdout);
    assert_eq!(doc.string_field("function").unwrap(), "Add2");
    assert!(
        doc.string_field("meaning_id")
            .unwrap()
            .starts_with("emath:meaning:v1:"),
        "artifact must carry the package meaning_id"
    );
    // Axes keep CLI order and raw values.
    let JsonValue::Arr(axes) = doc.field("grid").unwrap().field("axes").unwrap() else {
        panic!("grid.axes must be an array");
    };
    assert_eq!(axes.len(), 2);
    assert_eq!(axes[0].string_field("name").unwrap(), "p");
    assert_eq!(axes[1].string_field("name").unwrap(), "z");
    let JsonValue::Arr(p_values) = axes[0].field("values").unwrap() else {
        panic!("axis values must be an array");
    };
    let p_values: Vec<String> = p_values
        .iter()
        .map(|value| match value {
            JsonValue::Str(text) => text.clone(),
            other => panic!("axis value must be a raw string, got {other:?}"),
        })
        .collect();
    assert_eq!(p_values, ["2", "3"]);
    // Cartesian expansion: first axis slowest, second fastest.
    let JsonValue::Arr(cells) = doc.field("cells").unwrap() else {
        panic!("cells must be an array");
    };
    assert_eq!(cells.len(), 4);
    let expected = [
        vec![("p", "2"), ("z", "10")],
        vec![("p", "2"), ("z", "20")],
        vec![("p", "3"), ("z", "10")],
        vec![("p", "3"), ("z", "20")],
    ];
    for (cell, want) in cells.iter().zip(expected) {
        let want: Vec<(String, String)> = want
            .into_iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect();
        assert_eq!(cell_pairs(cell, "bindings"), want);
        assert_eq!(cell_status(cell), "ok");
    }
    // Cell values come from the same reference-VM path as eval.
    let sums: Vec<String> = cells
        .iter()
        .map(|cell| {
            cell_pairs(cell, "outputs")
                .into_iter()
                .find(|(name, _)| name == "s")
                .expect("output s")
                .1
        })
        .collect();
    assert_eq!(sums, ["12", "22", "13", "23"]);
    let summary = doc.field("summary").unwrap();
    assert_eq!(summary.int_field("total").unwrap(), 4);
    assert_eq!(summary.int_field("ok").unwrap(), 4);
    assert_eq!(summary.int_field("mismatch").unwrap(), 0);
    assert_eq!(summary.int_field("error").unwrap(), 0);

    // Single axis: values in CLI order.
    let sq = write_fixture(&dir, "sq1.emath", SQ1);
    let sq = sq.to_string_lossy().into_owned();
    let (stdout, _stderr, code) = sweep(&sq, &["--function", "Sq", "--grid", "p=2,3,5", "--json"]);
    expect_exit(code, EXIT_OK);
    let doc = parse_artifact(&stdout);
    let JsonValue::Arr(cells) = doc.field("cells").unwrap() else {
        panic!("cells must be an array");
    };
    let squares: Vec<String> = cells
        .iter()
        .map(|cell| {
            cell_pairs(cell, "outputs")
                .into_iter()
                .find(|(name, _)| name == "y")
                .expect("output y")
                .1
        })
        .collect();
    assert_eq!(squares, ["4", "9", "25"]);
}

/// Malformed grids and unusable selections are typed refusals, never
/// silent partial sweeps.
#[test]
fn sweep_grid_parser_rejects_malformed() {
    let dir = fixture_dir("malformed");
    let path = write_fixture(&dir, "add2.emath", ADD2);
    let path = path.to_string_lossy().into_owned();
    let add2 = path.as_str();

    // Malformed axis specs and shape errors are usage (exit 2).
    for args in [
        vec!["--function", "Add2", "--grid", "p"],
        vec!["--function", "Add2", "--grid", "p="],
        vec!["--function", "Add2", "--grid", "=1,2"],
        vec!["--function", "Add2", "--grid", "p=1,,2"],
        vec!["--function", "Add2", "--grid", "p=1,2", "--grid", "p=3"],
        vec!["--function", "Add2"],
        vec!["--grid", "p=1,2", "z=3"],
        vec!["--function", "Add2", "--grid", "p=1,2", add2, add2],
        vec!["--function", "Add2", "--grid", "p=1,2", "--expect", "s"],
        vec![
            "--function",
            "Add2",
            "--grid",
            "p=1,2",
            "--expect",
            "s=11",
            "--expect",
            "s=12",
        ],
    ] {
        let (_stdout, stderr, code) = sweep(add2, &args);
        expect_exit(code, EXIT_USAGE);
        assert!(
            stderr.contains("usage: emath sweep"),
            "malformed invocation must print the sweep usage, got: {stderr}"
        );
    }

    // A grid axis naming a non-input is a typed E-EVAL-005 refusal.
    let (stdout, _stderr, code) = sweep(add2, &["--function", "Add2", "--grid", "q=1,2", "--json"]);
    expect_exit(code, EXIT_REFUSED);
    let doc = parse_artifact(&stdout);
    let JsonValue::Arr(diagnostics) = doc.field("diagnostics").unwrap() else {
        panic!("diagnostics must be an array");
    };
    assert_eq!(diagnostics[0].string_field("code").unwrap(), "E-EVAL-005");

    // A grid that leaves a declared input unbound is E-EVAL-004.
    let (_stdout, _stderr, code) = sweep(add2, &["--function", "Add2", "--grid", "p=1,2"]);
    expect_exit(code, EXIT_REFUSED);

    // A grid value that cannot parse for the declared input type is E-EVAL-005.
    let (stdout, _stderr, code) = sweep(
        add2,
        &["--function", "Add2", "--grid", "p=x", "z=3", "--json"],
    );
    expect_exit(code, EXIT_REFUSED);
    let doc = parse_artifact(&stdout);
    let JsonValue::Arr(diagnostics) = doc.field("diagnostics").unwrap() else {
        panic!("diagnostics must be an array");
    };
    assert_eq!(diagnostics[0].string_field("code").unwrap(), "E-EVAL-005");

    // Unknown entrypoint and unknown expect output are typed refusals.
    let (stdout, _stderr, code) = sweep(
        add2,
        &["--function", "Nope", "--grid", "p=1", "z=2", "--json"],
    );
    expect_exit(code, EXIT_REFUSED);
    let doc = parse_artifact(&stdout);
    let JsonValue::Arr(diagnostics) = doc.field("diagnostics").unwrap() else {
        panic!("diagnostics must be an array");
    };
    assert_eq!(diagnostics[0].string_field("code").unwrap(), "E-EVAL-002");

    let (stdout, _stderr, code) = sweep(
        add2,
        &[
            "--function",
            "Add2",
            "--grid",
            "p=1",
            "z=2",
            "--expect",
            "nope=1",
            "--json",
        ],
    );
    expect_exit(code, EXIT_REFUSED);
    let doc = parse_artifact(&stdout);
    let JsonValue::Arr(diagnostics) = doc.field("diagnostics").unwrap() else {
        panic!("diagnostics must be an array");
    };
    assert_eq!(diagnostics[0].string_field("code").unwrap(), "E-EVAL-005");
}

/// Expectations gate every cell; flipping one `--expect` flips exactly
/// that cell's status (the dispatch's mutation check), and the human
/// line format matches the proximity-prize sweep ledger byte-for-byte.
#[test]
fn sweep_expectation_checking_and_mutation_flip() {
    let dir = fixture_dir("expectations");
    let path = write_fixture(&dir, "add2.emath", ADD2);
    let path = path.to_string_lossy().into_owned();
    let add2 = path.as_str();

    // s = p + z: cell (1,10) matches 11, cell (2,10) does not.
    let (stdout, _stderr, code) = sweep(
        add2,
        &[
            "--function",
            "Add2",
            "--grid",
            "p=1,2",
            "z=10",
            "--expect",
            "s=11",
        ],
    );
    expect_exit(code, EXIT_REFUSED);
    assert_eq!(
        stdout,
        "Add2 p=1 z=10: 11 OK\nAdd2 p=2 z=10: 12 MISMATCH (want 11)\n"
    );

    // Flip the expectation: the failing cell flips, the passing one stays.
    let (stdout_json, _stderr, code) = sweep(
        add2,
        &[
            "--function",
            "Add2",
            "--grid",
            "p=1,2",
            "z=10",
            "--expect",
            "s=12",
            "--json",
        ],
    );
    expect_exit(code, EXIT_REFUSED);
    let doc = parse_artifact(&stdout_json);
    let JsonValue::Arr(cells) = doc.field("cells").unwrap() else {
        panic!("cells must be an array");
    };
    assert_eq!(cell_status(&cells[0]), "mismatch");
    assert_eq!(cells[0].string_field("want").unwrap(), "12");
    assert_eq!(cells[0].string_field("got").unwrap(), "11");
    assert_eq!(cell_status(&cells[1]), "ok");
    let summary = doc.field("summary").unwrap();
    assert_eq!(summary.int_field("mismatch").unwrap(), 1);

    // An expectation that every cell satisfies exits 0.
    let (stdout, _stderr, code) = sweep(
        add2,
        &[
            "--function",
            "Add2",
            "--grid",
            "p=1",
            "z=10",
            "--expect",
            "s=11",
        ],
    );
    expect_exit(code, EXIT_OK);
    assert_eq!(stdout, "Add2 p=1 z=10: 11 OK\n");

    // Multiple expectations: values render in expect order; the first
    // failing want names the mismatch.
    let dual = write_fixture(&dir, "dual.emath", DUAL);
    let dual = dual.to_string_lossy().into_owned();
    let (stdout, _stderr, code) = sweep(
        &dual,
        &[
            "--function",
            "Dual",
            "--grid",
            "p=2",
            "z=10",
            "--expect",
            "s=12",
            "--expect",
            "m=20",
        ],
    );
    expect_exit(code, EXIT_OK);
    assert_eq!(stdout, "Dual p=2 z=10: 12 20 OK\n");
    let (stdout, _stderr, code) = sweep(
        &dual,
        &[
            "--function",
            "Dual",
            "--grid",
            "p=2",
            "z=10",
            "--expect",
            "s=12",
            "--expect",
            "m=21",
        ],
    );
    expect_exit(code, EXIT_REFUSED);
    assert_eq!(stdout, "Dual p=2 z=10: 12 20 MISMATCH (want 21)\n");
}

/// The artifact is a deterministic byte stream: identical invocations
/// produce identical stdout and identical `--out` files, and no
/// wall-clock field ever appears.
#[test]
fn sweep_artifact_determinism() {
    let dir = fixture_dir("determinism");
    let path = write_fixture(&dir, "sq1.emath", SQ1);
    let path = path.to_string_lossy().into_owned();
    let sq = path.as_str();

    let (first, _stderr, code) = sweep(sq, &["--function", "Sq", "--grid", "p=2,3,5", "--json"]);
    expect_exit(code, EXIT_OK);
    let (second, _stderr, code) = sweep(sq, &["--function", "Sq", "--grid", "p=2,3,5", "--json"]);
    expect_exit(code, EXIT_OK);
    assert_eq!(first, second, "identical sweeps must emit identical bytes");

    for banned in ["timestamp", "elapsed", "duration", "generated_at", "now"] {
        assert!(
            !first.to_ascii_lowercase().contains(banned),
            "artifact must not carry wall-clock field {banned}"
        );
    }

    let out_a = dir.join("a.json");
    let out_b = dir.join("b.json");
    let out_a_str = out_a.to_string_lossy().into_owned();
    let out_b_str = out_b.to_string_lossy().into_owned();
    let (stdout_a, _stderr, code) = sweep(
        sq,
        &["--function", "Sq", "--grid", "p=2,3,5", "--out", &out_a_str],
    );
    expect_exit(code, EXIT_OK);
    let (stdout_b, _stderr, code) = sweep(
        sq,
        &["--function", "Sq", "--grid", "p=2,3,5", "--out", &out_b_str],
    );
    expect_exit(code, EXIT_OK);
    let file_a = std::fs::read_to_string(&out_a).expect("artifact file a");
    let file_b = std::fs::read_to_string(&out_b).expect("artifact file b");
    assert_eq!(file_a, file_b, "--out artifacts must be byte-identical");
    assert_eq!(stdout_a, stdout_b);
    let doc = parse_artifact(&file_a);
    assert!(
        doc.string_field("meaning_id")
            .unwrap()
            .starts_with("emath:meaning:v1:")
    );
    let JsonValue::Arr(cells) = doc.field("cells").unwrap() else {
        panic!("cells must be an array");
    };
    assert_eq!(cells.len(), 3);
}

/// Acceptance: the sweep lane reproduces every row of
/// internal/proximity-prize/sweep-results.txt exactly on
/// powerword-zero-sum.emath — same values, same statuses, same order.
#[test]
fn sweep_acceptance_proximity_prize_reproduction() {
    let results = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../internal/proximity-prize/sweep-results.txt"),
    )
    .expect("read sweep-results.txt");
    let powerword = powerword();
    let powerword = powerword.to_string_lossy().into_owned();

    let mut replayed = 0;
    for line in results.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("EXIT=") {
            continue;
        }
        // Row grammar: `<fn> p=<P> z=<Z>: <value> OK`.
        let Some((function, rest)) = trimmed.split_once(" p=") else {
            panic!("unparsed results row: {trimmed}");
        };
        let Some((p_value, rest)) = rest.split_once(" z=") else {
            panic!("unparsed results row: {trimmed}");
        };
        let Some((z_value, tail)) = rest.split_once(": ") else {
            panic!("unparsed results row: {trimmed}");
        };
        let Some((value, "OK")) = tail.split_once(' ') else {
            panic!("results row is not an OK row: {trimmed}");
        };
        let (stdout, _stderr, code) = sweep(
            &powerword,
            &[
                "--function",
                function,
                "--grid",
                &format!("p={p_value}"),
                &format!("z={z_value}"),
            ],
        );
        expect_exit(code, EXIT_OK);
        assert_eq!(
            stdout,
            format!("{trimmed}\n"),
            "sweep row must reproduce the proximity-prize artifact exactly"
        );
        let _ = value;
        replayed += 1;
    }
    assert_eq!(replayed, 15, "every results row must replay");

    // Cartesian mode over the flagship instance: the diagonal cells
    // (17,9) and (41,27) reproduce their rows; the off-diagonal cells
    // enumerate deterministically.
    let (stdout, _stderr, code) = sweep(
        &powerword,
        &["--function", "n1_k3_n8", "--grid", "p=17,41", "z=9,27"],
    );
    expect_exit(code, EXIT_OK);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 4);
    assert_eq!(lines[0], "n1_k3_n8 p=17 z=9: 6 OK");
    assert_eq!(lines[3], "n1_k3_n8 p=41 z=27: 6 OK");
}
