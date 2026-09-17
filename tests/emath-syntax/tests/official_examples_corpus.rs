//! Constructor teaching examples run.

use emath_cli::{run, CliExit};
use emath_test_harness::{boot, demand_workspace_corpora, Probe};

#[test]
fn corpora_and_language_gaps() {
    let mut probe = Probe::new("constructor teaching examples run");
    probe.case("teaching_cli_oracles", |probe| {
        boot();
        for (name, rel, args, expected) in [
            (
                "run heat",
                "language/examples/applied/heat.emath",
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
                "check object-basics",
                "language/examples/objects/object-basics.emath",
                &["check"][..],
                CliExit::Ok,
            ),
            (
                "check sequence-fold",
                "language/examples/recursion/sequence-fold.emath",
                &["check"][..],
                CliExit::Ok,
            ),
            (
                "check quote-view",
                "language/examples/code/quote-view.emath",
                &["check"][..],
                CliExit::Ok,
            ),
            (
                "check quote-substitute",
                "language/examples/code/quote-substitute.emath",
                &["check"][..],
                CliExit::Ok,
            ),
            (
                "check certified-enclosure",
                "language/examples/queries/certified-enclosure.emath",
                &["check"][..],
                CliExit::Ok,
            ),
            (
                "check zero-normalizer",
                "language/examples/queries/zero-normalizer.emath",
                &["check"][..],
                CliExit::Ok,
            ),
            (
                "check code-answer",
                "language/examples/queries/code-answer.emath",
                &["check"][..],
                CliExit::Ok,
            ),
        ] {
            let path = emath_test_harness::workspace_path(rel);
            let mut argv: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
            argv.insert(1.min(argv.len()), path.to_string_lossy().into_owned());
            probe.eq(name, run(&argv), expected);
        }
    });
    probe.case("workspace_corpora", |probe| {
        demand_workspace_corpora(probe);
    });
    probe.finish();
}
