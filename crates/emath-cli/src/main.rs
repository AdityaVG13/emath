//! emath CLI entry point. All logic lives in the library for testability.

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match emath_cli::run(&args) {
        0 => ExitCode::SUCCESS,
        1 => ExitCode::from(1),
        _ => ExitCode::from(2),
    }
}
