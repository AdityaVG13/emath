//! `emath sweep` is not a constructor command.
mod common;
use emath_cli::EXIT_USAGE;
use emath_test_harness::Probe;

fn sweep(path: &str, args: &[&str]) -> (String, i32) {
    let o = std::process::Command::new(common::emath_lab_bin())
        .arg("sweep")
        .arg(path)
        .args(args)
        .output()
        .expect("sweep");
    (
        String::from_utf8_lossy(&o.stderr).into_owned(),
        o.status.code().unwrap_or(-1),
    )
}

#[test]
fn probe() {
    let mut p = Probe::new("sweep refuses; cartesian grids are not a second language");
    let (err, code) = sweep("add2.emath", &["--function", "Add2", "--grid", "p=2,3", "--json"]);
    p.eq("exit", code, EXIT_USAGE as i32);
    p.contains("kind", &err, "E-KIND-GONE");
    p.finish();
}
