#![forbid(unsafe_code)]
//! Compiled function-spec probe battery (emath-bta82).
//!
//! `BuildOptions::bin_entrypoint` emits a standalone sibling binary with
//! the same `--set` CLI contract as `emath eval`. The interpreter is the
//! reference semantics; these tests pin the COMPILED path to the same
//! hand-derived values the interpreter suites pin (`witness_sum`,
//! `v[0]*10+v[1] == 34`, is also pinned on the eval side in
//! tests/emath-cli/tests/eval_function_specs.rs), so parity is enforced
//! by shared expectations, not by shelling out to the CLI.
//!
//! Discrimination: the refusal-contract test fails against a shim that
//! drops any typed check (undeclared-name, duplicate, fractional Int,
//! negative Nat, unknown argument); the parity tests fail against a
//! display or parse drift (the `.0` float suffix, vector element echo).

use emath_build::{BuildOptions, build_text};
use std::path::PathBuf;
use std::process::Command;

const BATTERY_SPEC: &str = "\
emath function census:
    inputs:
        n: Int
    outputs:
        c: Int
    definitions:
        c = sum i in 0..n: pow_mod(i * i + 1, i, 29)

emath function witness:
    inputs:
        v: Vector[Int]
    outputs:
        m: Int
    definitions:
        m = v[0] * 10 + v[1]

emath function offset:
    inputs:
        x: Float64
    outputs:
        y: Float64
    definitions:
        y = x + 0.5

emath function natz:
    inputs:
        k: Nat
    outputs:
        r: Int
    definitions:
        r = k + 1
";

/// Build `BATTERY_SPEC` with `entrypoint` compiled as a probe under a
/// fresh scratch output dir; returns the probe binary path.
fn build_probe(entrypoint: &str, tag: &str) -> PathBuf {
    let out = std::env::temp_dir().join(format!("emath-bta82-{tag}"));
    let _ = std::fs::remove_dir_all(&out);
    let report = build_text(
        "probe_lab.emath",
        BATTERY_SPEC,
        &out,
        BuildOptions {
            bin_entrypoint: Some(entrypoint.to_string()),
            ..BuildOptions::default()
        },
    )
    .expect("battery spec must build with a compiled probe");
    report
        .probe_binary
        .unwrap_or_else(|| panic!("--bin {entrypoint} must produce a probe binary"))
}

/// Run the probe with `--set` bindings; returns (exit code, stdout).
fn run_probe(binary: &std::path::Path, sets: &[&str]) -> (Option<i32>, String, String) {
    let mut command = Command::new(binary);
    let mut index = 0;
    while index < sets.len() {
        command.arg("--set").arg(sets[index]);
        index += 1;
    }
    let output = command.output().expect("probe binary must run");
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn probe_census_parity_battery() {
    let binary = build_probe("census", "census");
    // Hand-derived: Σ_{i<n} (i²+1 mod 29)^i mod 29 … term-by-term:
    // i=0 → 1^0=1; i=1 → 2^1=2; i=2 → 5^2=25; i=3 → 10^3 mod 29 = 14.
    // Sums: 0, 1, 3, 28, 42 (the same values the interpreter eval pins
    // for this exact spec).
    for (n, expected) in [(0, "0"), (1, "1"), (2, "3"), (3, "28"), (4, "42")] {
        let (code, stdout, stderr) = run_probe(&binary, &[&format!("n={n}")]);
        assert_eq!(code, Some(0), "census n={n}: {stderr}");
        let last = stdout.lines().last().unwrap_or_default();
        assert_eq!(
            last,
            format!("output c = {expected}"),
            "compiled census must match the interpreter value for n={n}"
        );
    }
}

#[test]
fn probe_receipt_and_input_echo_contract() {
    let binary = build_probe("census", "receipt");
    let (_, stdout, _) = run_probe(&binary, &["n=29"]);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.first().copied(), Some("inputs_from set"));
    let receipt = lines
        .iter()
        .find(|line| line.starts_with("receipt "))
        .expect("probe stdout must carry a receipt line");
    assert!(
        receipt.contains("engine=compiled-probe"),
        "receipt names its engine: {receipt}"
    );
    assert!(
        receipt.contains("meaning_id=emath:meaning:v1:"),
        "receipt ships the build-time meaning id: {receipt}"
    );
    assert!(
        receipt.contains("inputs_hash=fnv1a64:"),
        "receipt ships the FNV-1a inputs hash: {receipt}"
    );
    assert!(
        receipt.contains("world=not-applicable-to-function-probes"),
        "world is structurally absent for function probes (E-EVAL-008), typed marker only: {receipt}"
    );
    assert!(
        receipt.contains("method=not-applicable-to-function-probes"),
        "method is structurally absent for function probes, typed marker only: {receipt}"
    );
    assert!(
        lines.contains(&"input n = 29"),
        "input echo mirrors the interpreter display: {stdout}"
    );
}

#[test]
fn probe_vector_and_float_shapes() {
    // Vector[Int] witness (Result-wrapped indexing path) — the same
    // value the interpreter pins in eval_function_specs: 3*10 + 4 = 34.
    let witness = build_probe("witness", "witness");
    let (code, stdout, stderr) = run_probe(&witness, &["v=[3, 4]"]);
    assert_eq!(code, Some(0), "{stderr}");
    assert!(
        stdout.contains("input v = [3.0, 4.0]"),
        "vector input echo mirrors Value::Display: {stdout}"
    );
    assert!(
        stdout.contains("output m = 34"),
        "compiled witness must match the interpreter: {stdout}"
    );

    // Float64 shape: display mirror must keep the `.0` integer-look
    // suffix exactly like format_f64 (1 → "1.0", 2.25 → "2.25").
    let offset = build_probe("offset", "offset");
    let (code, stdout, _) = run_probe(&offset, &["x=2.25"]);
    assert_eq!(code, Some(0));
    assert!(
        stdout.contains("input x = 2.25") && stdout.contains("output y = 2.75"),
        "float passthrough parity: {stdout}"
    );
    let (code, stdout, _) = run_probe(&offset, &["x=1"]);
    assert_eq!(code, Some(0));
    assert!(
        stdout.contains("input x = 1.0") && stdout.contains("output y = 1.5"),
        "float display must carry the .0 suffix like the interpreter: {stdout}"
    );
}

#[test]
fn probe_refusal_contract() {
    let binary = build_probe("census", "refusal-int");
    // Every refusal: nonzero exit, ONE actionable stderr line, no value.
    let cases: &[(&[&str], &str)] = &[
        (&[], "missing input `n`"),
        (&["n=29", "n=30"], "duplicate `--set` binding"),
        (&["m=29"], "undeclared input `m`"),
        (&["n=2.5"], "not an exact integer"),
    ];
    for (sets, expected) in cases {
        let (code, stdout, stderr) = run_probe(&binary, sets);
        assert_ne!(code, Some(0), "must refuse {sets:?}");
        assert!(
            stdout
                .lines()
                .last()
                .is_none_or(|l| !l.starts_with("output")),
            "no value line on refusal: {stdout}"
        );
        let stderr = stderr.trim();
        assert_eq!(
            stderr.lines().count(),
            1,
            "refusal is one actionable line: {stderr:?}"
        );
        assert!(
            stderr.contains(expected),
            "refusal must name the cause {expected:?}: {stderr:?}"
        );
    }
    // Unknown argument and bare --set.
    let extra_cases: &[&[&str]] = &[&["--bogus"], &["--set", "--set"]];
    for extra in extra_cases {
        let mut command = Command::new(&binary);
        command.args(extra.iter());
        let output = command.output().expect("probe binary must run");
        assert_ne!(output.status.code(), Some(0), "must refuse {extra:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(
            stderr.trim().lines().count(),
            1,
            "refusal is one actionable line: {stderr:?}"
        );
    }

    // Nat strictness on a dedicated entrypoint: negative and fractional
    // both refuse typed.
    let natz = build_probe("natz", "refusal-nat");
    for binding in ["k=-3", "k=1.5"] {
        let (code, _stdout, stderr) = run_probe(&natz, &[binding]);
        assert_ne!(code, Some(0), "Nat must refuse {binding}");
        let stderr = stderr.trim();
        assert!(
            stderr.contains("non-negative") || stderr.contains("not an exact integer"),
            "Nat refusal is typed: {stderr:?}"
        );
    }

    // Vector[Int] element strictness mirrors the eval gate: fractional
    // element refuses.
    let witness = build_probe("witness", "refusal-vec");
    let (code, _stdout, stderr) = run_probe(&witness, &["v=[3.5, 4]"]);
    assert_ne!(code, Some(0), "Vector[Int] must refuse 3.5");
    assert!(
        stderr.contains("does not match the declared input type"),
        "vector element refusal is typed: {stderr:?}"
    );
}
