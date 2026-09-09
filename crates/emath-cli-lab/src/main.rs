//! `emath-lab` entry: extracted lab/host commands (emath-qbk53).

#![forbid(unsafe_code)]

use std::process::ExitCode;

fn main() -> ExitCode {
    emath_syntax::install_source_parser();
    let args: Vec<String> = std::env::args().skip(1).collect();
    match emath_cli_lab::run(&args) {
        emath_cli::CliExit::Ok => ExitCode::SUCCESS,
        emath_cli::CliExit::Refused => ExitCode::from(1),
        emath_cli::CliExit::Usage => ExitCode::from(2),
    }
}
