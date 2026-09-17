//! `emath check --verify-data` (04 §5.2) on the constructor surface.
//!
//! The observation-evidence fixtures (`science/observations.emath`,
//! `observations_hash_drift.emath`, `observations_write.emath`) were
//! pruned with the recipe lane, so the drift/write negative lanes they
//! pinned are gone with them. What remains live: `--verify-data` on a
//! constructor file with no declared data files has nothing to hash and
//! admits, exactly like a plain check.

mod common;
use common::cli;
use emath_test_harness::Probe;
fn repo(rel: &str) -> String { std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").join(rel).to_str().expect("utf8").to_string() }
#[test]
fn probe() {
    let mut p = Probe::new("verify-data admits data-less constructor files; plain check never hashes");
    let dataless = repo("tests/valid/formatter_square.emath");
    p.case("verify-data", |p| { let (out, code) = cli(&["check", "--verify-data", &dataless]); p.eq("exit", code, 0); p.demand("no-hash-code", !out.contains("E-OBS-HASH"), "no declared data means nothing to hash"); });
    p.case("plain", |p| { let (out, code) = cli(&["check", &dataless]); p.eq("exit", code, 0); p.demand("admits", !out.contains("E-OBS"), "plain check admits the same file"); });
    p.finish();
}
