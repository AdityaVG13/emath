//! Authored constructor tests. Leftover generated-crate execution stays
//! on disk (RULE 1) and is not dispatched.

use super::{Path, CliExit, EXIT_USAGE, print_diagnostics, EXIT_ADMISSION, EXIT_REFUSED, EXIT_OK, EXIT_FAULT};

/// Run `tests:` examples through the constructor evaluator. `work`
/// raises the per-module budget (`--work N`), so large-instance rows
/// are pinnable instead of refusing `budget_exhausted` at the default.
pub(crate) fn test_cmd(file: &Path, _out: &Path, work: Option<u64>) -> CliExit {
    if let Some(code) = crate::refuse_malformed_project_lock(file) {
        return code;
    }
    let source = match std::fs::read_to_string(file) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("error: E-PKG-080: {error}");
            return EXIT_USAGE;
        }
    };
    let (tree, diagnostics) = emath_syntax::parse_str(&source);
    if diagnostics.has_errors() {
        print_diagnostics(&diagnostics);
        return EXIT_ADMISSION;
    }
    let report = match work {
        Some(limit) => emath_exec_ir::constructor_layer::evaluate_tree_budgeted_at(
            &tree,
            Some(file),
            limit,
        ),
        None => emath_exec_ir::constructor_layer::evaluate_tree_at(&tree, Some(file)),
    };
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
            eprintln!("error: {}: {}", error.code, error.message);
            // Shape refusals (not-a-constructor kinds, unrecognized test
            // rows) are admission failures, not execution faults.
            if error.code == "E-KIND-GONE" || error.code == "unknown_test_row" {
                EXIT_ADMISSION
            } else {
                EXIT_FAULT
            }
        }
    }
}
