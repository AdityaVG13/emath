//! expand / exactness / freeze / plan / agent are extracted tokens.
use emath_cli::EXIT_USAGE;
use emath_cli_lab::run;
use emath_test_harness::{Probe, boot};

fn repo(rel: &str) -> String {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel)
        .to_string_lossy()
        .into_owned()
}

#[test]
fn probe() {
    boot();
    let mut p = Probe::new("expand, exactness, freeze, plan, and agent refuse");
    let scratch = repo("tests/fixtures/language/intro/scratch.emath");
    let hello = repo("tests/fixtures/language/intro/hello-square.emath");
    for argv in [
        vec!["expand", scratch.as_str()],
        vec!["expand", scratch.as_str(), "--json"],
        vec!["exactness", scratch.as_str()],
        vec!["freeze", scratch.as_str()],
        vec!["assumptions", scratch.as_str()],
        vec!["why", scratch.as_str(), "inference:1"],
        vec!["plan", hello.as_str(), "--json"],
        vec!["agent", "plan", hello.as_str()],
        vec!["agent", "check", scratch.as_str()],
    ] {
        p.eq(
            argv.join(" "),
            run(&argv.iter().map(|s| s.to_string()).collect::<Vec<_>>()),
            EXIT_USAGE,
        );
    }
    p.finish();
}
