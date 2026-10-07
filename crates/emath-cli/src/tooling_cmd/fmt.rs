//! The `emath fmt` formatting command.

use super::{CliExit, EXIT_ADMISSION, EXIT_REFUSED, EXIT_OK, Path, CompilerSession, EXIT_IO};

pub(crate) fn fmt_cmd(file: &Path) -> CliExit {
    if file.as_os_str() == "-" {
        return fmt_stdin();
    }
    let mut session = CompilerSession::new(emath_core::limits::Limits::default());
    let Ok(package) = session.load_package(file) else {
        eprintln!("error: cannot read {}", file.display());
        return EXIT_IO;
    };
    let result = session.check(package.file);
    crate::cli_json::print_diagnostics_from(&result.diagnostics, &session.store);
    if result.diagnostics.has_errors() {
        return EXIT_ADMISSION;
    }
    let limits = emath_core::limits::Limits::default();
    let lossless = emath_syntax::parse_lossless(&package.text, package.file, &limits);
    let canonical = emath_syntax::format(&lossless.tree, &lossless.comments);
    if canonical == package.text {
        println!(
            "fmt: {}: canonical form (lossless round-trip)",
            file.display()
        );
        EXIT_OK
    } else {
        eprintln!(
            "fmt: {}: NOT canonical; expected lossless formatter output",
            file.display()
        );
        for (line_no, (expected, actual)) in canonical
            .lines()
            .zip(package.text.lines())
            .enumerate()
            .filter(|(_, (expected, actual))| expected != actual)
            .take(10)
        {
            eprintln!(
                "  line {}: expected `{expected}`, found `{actual}`",
                line_no + 1
            );
        }
        EXIT_REFUSED
    }
}

/// `fmt -`: canonical-form check on stdin (pipelines). Same lossless
/// round-trip contract as file mode: stdin is never rewritten, parse
/// errors refuse before the round-trip comparison, and a non-canonical
/// diff is reported on stderr.
fn fmt_stdin() -> CliExit {
    use std::io::Read;
    let mut source = String::new();
    if std::io::stdin().read_to_string(&mut source).is_err() {
        eprintln!("error: cannot read `.emath` source from stdin");
        return EXIT_IO;
    }
    let checked = crate::cli_build::checked_text("", &source);
    if checked.result.diagnostics.has_errors() {
        crate::cli_json::print_diagnostics_from(&checked.result.diagnostics, &checked.store);
        return EXIT_ADMISSION;
    }
    let limits = emath_core::limits::Limits::default();
    let lossless = emath_syntax::parse_lossless(&source, emath_core::FileId(0), &limits);
    if lossless.diagnostics.has_errors() {
        crate::cli_json::print_diagnostics_in(&lossless.diagnostics, "<stdin>", &source);
        return EXIT_ADMISSION;
    }
    let canonical = emath_syntax::format(&lossless.tree, &lossless.comments);
    if canonical == source {
        println!("fmt: <stdin>: canonical form (lossless round-trip)");
        EXIT_OK
    } else {
        eprintln!("fmt: <stdin>: NOT canonical; expected lossless formatter output");
        for (line_no, (expected, actual)) in canonical
            .lines()
            .zip(source.lines())
            .enumerate()
            .filter(|(_, (expected, actual))| expected != actual)
            .take(10)
        {
            eprintln!(
                "  line {}: expected `{expected}`, found `{actual}`",
                line_no + 1
            );
        }
        EXIT_REFUSED
    }
}
