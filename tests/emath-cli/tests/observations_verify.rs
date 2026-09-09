//! `emath check --verify-data` (04 §5.2).
mod common;
use common::cli;
use emath_test_harness::Probe;
fn repo(rel: &str) -> String { std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").join(rel).to_str().expect("utf8").to_string() }
#[test]
fn probe() {
    let mut p = Probe::new("verify-data hashes declared evidence, plain check never hashes");
    p.case("match", |p| { let (out, code) = cli(&["check", "--verify-data", &repo("tests/fixtures/language/science/observations.emath")]); p.eq("exit", code, 0); p.demand("no-hash-code", !out.contains("E-OBS-HASH"), "matching digest passes"); });
    p.case("drift", |p| { let (out, code) = cli(&["check", "--verify-data", &repo("tests/invalid/observations_hash_drift.emath")]); p.eq("exit", code, 1); p.contains("code", &out, "E-OBS-HASH"); });
    p.case("plain", |p| { let (out, code) = cli(&["check", &repo("tests/invalid/observations_hash_drift.emath")]); p.eq("exit", code, 0); p.demand("declared-not-verified", !out.contains("E-OBS-HASH"), "plain check never hashes"); });
    p.case("write", |p| { let (out, code) = cli(&["check", &repo("tests/invalid/observations_write.emath")]); p.eq("exit", code, 1); p.contains("code", &out, "E-OBS-WRITE"); });
    p.finish();
}
