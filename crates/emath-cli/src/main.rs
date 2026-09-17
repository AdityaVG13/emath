//! emath CLI entry point. All logic lives in the library for testability.

#![forbid(unsafe_code)]

use std::process::ExitCode;

fn main() -> ExitCode {
    // Install the source-parser backend once per process.
    emath_syntax::install_source_parser();
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Evaluation recurses natively per authored call (~50 KiB of stack each
    // at the observed worst case), so the documented call-depth budget of
    // 256 must fire as `recursion_depth_exceeded` well before the process
    // stack ends. The default 8 MiB main-thread stack dies near 200 nested
    // calls; 64 MiB covers 256 frames plus expression-nesting headroom.
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
