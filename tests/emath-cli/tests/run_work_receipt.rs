//! `emath run --json` receipts carry work-consumed telemetry (bead
//! emath-7zplf): tuning `--work` needs the measured consumption, not
//! just the raised limit. Failure-first: the envelope carried no work
//! datum at all before the fix.

mod common;
use emath_cli::EXIT_OK;
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
