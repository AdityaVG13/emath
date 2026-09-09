#![forbid(unsafe_code)]
//! Compiled function-spec probe battery: the interpreter is the reference, the compiled probe must match it.
use emath_build::{BuildOptions, build_text};
use emath_test_harness::Probe;
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

fn build_probe(entrypoint: &str, tag: &str) -> PathBuf {
    let out = std::env::temp_dir().join(format!("emath-bta82-{tag}"));
    let _ = std::fs::remove_dir_all(&out);
    let report = build_text("probe_lab.emath", BATTERY_SPEC, &out, BuildOptions { bin_entrypoint: Some(entrypoint.to_string()), ..BuildOptions::default() })
        .expect("battery spec must build");
    report.probe_binary.unwrap_or_else(|| panic!("--bin {entrypoint} must produce a probe binary"))
}
fn run_probe(binary: &std::path::Path, sets: &[&str]) -> (Option<i32>, String, String) {
    let mut cmd = Command::new(binary);
    for s in sets {
        cmd.arg("--set").arg(s);
    }
    let out = cmd.output().expect("probe must run");
    (out.status.code(), String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned())
}

#[test]
fn probe() {
    let mut p = Probe::new("compiled probes match the interpreter values, receipts, and refusal contract");
    let (census, witness, offset, natz) = (build_probe("census", "census"), build_probe("witness", "witness"), build_probe("offset", "offset"), build_probe("natz", "natz"));
    p.case("census", |p| {
        for (n, expected) in [(0, "0"), (1, "1"), (2, "3"), (3, "28"), (4, "42")] {
            let (code, stdout, stderr) = run_probe(&census, &[&format!("n={n}")]);
            p.eq(format!("n={n}/code"), code, Some(0));
            if code != Some(0) {
                p.fail(format!("n={n}/stderr"), stderr);
                continue;
            }
            p.eq(format!("n={n}/value"), stdout.lines().last().unwrap_or_default().to_string(), format!("output c = {expected}"));
        }
    });
    p.case("receipt", |p| {
        let (_, stdout, _) = run_probe(&census, &["n=29"]);
        let lines: Vec<&str> = stdout.lines().collect();
        p.eq("inputs-from", lines.first().copied(), Some("inputs_from set"));
        let receipt = lines.iter().find(|l| l.starts_with("receipt ")).expect("receipt line").to_string();
        for needle in ["engine=compiled-probe", "meaning_id=emath:meaning:v1:", "inputs_hash=fnv1a64:", "world=not-applicable-to-function-probes", "method=not-applicable-to-function-probes"] {
            p.contains(format!("receipt/{needle}"), &receipt, needle);
        }
        p.demand("echo", lines.contains(&"input n = 29"), format!("input echo: {stdout}"));
    });
    p.case("shapes", |p| {
        let (code, stdout, stderr) = run_probe(&witness, &["v=[3, 4]"]);
        p.eq("witness/code", code, Some(0));
        p.contains("witness/echo", &stdout, "input v = [3.0, 4.0]");
        p.contains("witness/value", &stdout, "output m = 34");
        if code != Some(0) {
            p.fail("witness/stderr", stderr);
        }
        let (code, stdout, _) = run_probe(&offset, &["x=2.25"]);
        p.eq("float/code", code, Some(0));
        p.demand("float", stdout.contains("input x = 2.25") && stdout.contains("output y = 2.75"), format!("passthrough: {stdout}"));
        let (code, stdout, _) = run_probe(&offset, &["x=1"]);
        p.eq("dot0/code", code, Some(0));
        p.demand("dot0", stdout.contains("input x = 1.0") && stdout.contains("output y = 1.5"), format!("dot0 suffix: {stdout}"));
    });
    p.case("refusals", |p| {
        for (sets, expected) in [(&[][..], "missing input `n`"), (&["n=29", "n=30"][..], "duplicate `--set` binding"), (&["m=29"][..], "undeclared input `m`"), (&["n=2.5"][..], "not an exact integer")] {
            let (code, stdout, stderr) = run_probe(&census, sets);
            p.ne(format!("{sets:?}/code"), code, Some(0));
            p.demand(format!("{sets:?}/no-value"), stdout.lines().last().is_none_or(|l| !l.starts_with("output")), format!("no value: {stdout}"));
            p.eq(format!("{sets:?}/lines"), stderr.trim().lines().count(), 1);
            p.contains(format!("{sets:?}/cause"), stderr.trim(), expected);
        }
        for extra in [&["--bogus"][..], &["--set", "--set"][..]] {
            let mut cmd = Command::new(&census);
            cmd.args(extra.iter());
            let out = cmd.output().expect("must run");
            p.ne(format!("{extra:?}/code"), out.status.code(), Some(0));
            p.eq(format!("{extra:?}/lines"), String::from_utf8_lossy(&out.stderr).trim().lines().count(), 1);
        }
        for binding in ["k=-3", "k=1.5"] {
            let (code, _, stderr) = run_probe(&natz, &[binding]);
            p.ne(format!("nat/{binding}/code"), code, Some(0));
            p.demand(format!("nat/{binding}/typed"), stderr.contains("non-negative") || stderr.contains("not an exact integer"), format!("typed: {stderr:?}"));
        }
        let (code, _, stderr) = run_probe(&witness, &["v=[3.5, 4]"]);
        p.ne("vec/code", code, Some(0));
        p.contains("vec/typed", &stderr, "does not match the declared input type");
    });
    p.finish();
}
