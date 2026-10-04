//! emath CLI entry point. All logic lives in the library for testability.

#![forbid(unsafe_code)]

use std::process::ExitCode;

fn main() -> ExitCode {
    // Install the source-parser backend once per process.
    emath_syntax::install_source_parser();
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Evaluation recurses natively per authored call and grows its stack
    // on the heap (`stacker`) at each call. Deep expression nesting
    // between calls does not pass a growth point, so the worker still
    // starts with a large stack for that headroom.
    let worker = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || run_cli(&args))
        .expect("spawn CLI worker thread");
    match worker.join() {
        Ok(code) => code,
        Err(_) => ExitCode::FAILURE,
    }
}

fn run_cli(args: &[String]) -> ExitCode {
    match emath_cli::run(args) {
        emath_cli::CliExit::Ok => ExitCode::SUCCESS,
        emath_cli::CliExit::Refused => ExitCode::from(1),
        emath_cli::CliExit::Usage => ExitCode::from(2),
        emath_cli::CliExit::Toolchain => ExitCode::from(3),
        emath_cli::CliExit::Io => ExitCode::from(4),
        emath_cli::CliExit::Safety => ExitCode::from(5),
    }
}
