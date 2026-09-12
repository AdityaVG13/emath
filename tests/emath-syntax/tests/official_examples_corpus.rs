//! Constructor teaching examples run.

use emath_cli::{run, CliExit};
use emath_test_harness::{boot, Probe};

#[test]
fn corpora_and_language_gaps() {
    let mut probe = Probe::new("constructor teaching examples run");
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
        ] {
            let path = emath_test_harness::workspace_path(rel);
            let mut argv: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
            argv.insert(1.min(argv.len()), path.to_string_lossy().into_owned());
            probe.eq(name, run(&argv), expected);
        }
    });
    probe.finish();
}
