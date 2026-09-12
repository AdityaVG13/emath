//! `eval --world` is not constructor surface.
use emath_cli::EXIT_USAGE;
use emath_test_harness::Probe;
use std::process::Command;

mod common;

#[test]
fn world_builtin_bridge() {
    let mut p = Probe::new("eval --world refuses; builtin worlds are not a second language");
    let output = Command::new(common::emath_lab_bin())
        .args(["eval", "glyphs.emath", "--world", "integer-ring", "--json"])
        .output()
        .expect("run emath-lab");
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    p.eq("code", output.status.code().unwrap_or(-1), EXIT_USAGE as i32);
    p.contains("kind", &stderr, "E-KIND-GONE");
    p.finish();
}
