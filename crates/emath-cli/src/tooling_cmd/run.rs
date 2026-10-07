//! Authored constructor tests. Leftover generated-crate execution stays
//! on disk (RULE 1) and is not dispatched.

use super::{Path, CliExit, EXIT_ADMISSION, EXIT_REFUSED, EXIT_OK, EXIT_FAULT};

/// Run `tests:` examples through the constructor evaluator. `work`
/// raises the per-example budget (`--work N`), so large-instance rows
/// are pinnable instead of refusing `budget_exhausted` at the default.
pub(crate) fn test_cmd(file: &Path, _out: &Path, work: Option<u64>) -> CliExit {
    let checked = crate::cli_build::checked_file(file);
    let source = checked.source();
    let diagnostics = &checked.result.diagnostics;
    let name = file.display().to_string();
    if diagnostics.has_errors() {
        crate::cli_json::print_diagnostics_from(diagnostics, &checked.store);
        return EXIT_ADMISSION;
    }
    let Some(module) = &checked.result.prepared else {
        return EXIT_ADMISSION;
    };
    let report = module.evaluate_tests(work.unwrap_or(emath_exec_ir::constructor_layer::DEFAULT_WORK));
    match report {
        Ok(report) => {
            let mut failed = 0;
            for test in &report.tests {
                if test.passed {
                    println!("ok {} -- {}", test.label, test.detail);
                } else {
                    failed += 1;
                    println!("failed {} -- {}", test.label, test.detail);
                }
            }
            if failed > 0 {
                eprintln!("error: {failed} authored test(s) failed");
                EXIT_REFUSED
            } else {
                println!("{} authored tests passed", report.tests.len());
                EXIT_OK
            }
        }
        Err(error) => {
            eprintln!("{}", crate::cli_json::error_text(&error.code, &error.message, &name, source, error.span));
            // Admission refusals carry `E-` codes (the test lane admits
            // like `check`); raw fault names are execution faults.
            if error.code.starts_with("E-") {
                EXIT_ADMISSION
            } else {
                EXIT_FAULT
            }
        }
    }
}
