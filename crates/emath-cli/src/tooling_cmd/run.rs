//! Generated-artifact test execution. Mathematical run commands use the reference runner.

use super::*;

/// Build and run the generated crate's source examples.
pub(crate) fn test_cmd(file: &Path, out: &Path) -> CliExit {
    if let Some(code) = crate::refuse_malformed_project_lock(file) {
        return code;
    }
    match build_file(
        file,
        out,
        BuildOptions {
            verify_generated_crate: true,
            ..BuildOptions::default()
        },
    ) {
        Ok(report) => {
            println!(
                "test: artifact {} crate '{}' tests passed",
                report.artifact_id.0, report.crate_name
            );
            EXIT_OK
        }
        Err(error) => {
            eprintln!("error: {error}");
            classify_build_error(&error)
        }
    }
}
