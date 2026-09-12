//! Authored constructor tests. Leftover generated-crate execution stays
//! on disk (RULE 1) and is not dispatched.

use super::*;

/// Run `tests:` examples through the constructor evaluator.
pub(crate) fn test_cmd(file: &Path, _out: &Path) -> CliExit {
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
    match emath_exec_ir::constructor_layer::evaluate_tree_at(&tree, Some(file)) {
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
            if error.code == "E-KIND-GONE" {
                EXIT_ADMISSION
            } else {
                EXIT_FAULT
            }
        }
    }
}
