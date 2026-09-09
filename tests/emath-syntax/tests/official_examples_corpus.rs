//! Workspace corpora and language-gap ratchet. One probe, every file.
//!
//! Admission-only walks are fluff. Valid math must evaluate `expect` to
//! Passed. Invalid files must emit their pinned codes. Language gaps (RK45,
//! slices, intervals) stay red until the engine upgrades.

use emath_cli::{CliExit, run};
use emath_test_harness::{Probe, boot, demand_language_gaps, demand_workspace_corpora};

#[test]
fn corpora_and_language_gaps() {
    let mut probe = Probe::new(
        "valid/examples evaluate expects; invalid emits pinned E-*; RK45/slice/interval are real",
    );
    demand_workspace_corpora(&mut probe);
    demand_language_gaps(&mut probe);
    probe.case("teaching_cli_oracles", |probe| {
        boot();
        for (name, rel, args, expected) in [
            (
                "run newton-second",
                "language/examples/physics/newton-second.emath",
                &["run"][..],
                CliExit::Ok,
            ),
            (
                "run add-exact",
                "language/examples/intro/add-exact.emath",
                &["run"][..],
                CliExit::Ok,
            ),
            (
                "explain newton-second",
                "language/examples/physics/newton-second.emath",
                &["explain", "--provenance"][..],
                CliExit::Ok,
            ),
            (
                "run scratch is a hole",
                "tests/fixtures/language/intro/scratch.emath",
                &["run"][..],
                CliExit::Refused,
            ),
        ] {
            let path = emath_test_harness::workspace_path(rel);
            let mut argv: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
            argv.insert(1.min(argv.len()), path.to_string_lossy().into_owned());
            probe.eq(name, run(&argv), expected);
        }
    });
    probe.finish();
}
