//! Production `emath` CLI (constructor layer): `check`, `run`, `step`,
//! `inspect`, `test`, `build`, `api`. Extracted tokens
//! (`eval`, `sweep`, `genesis`, `solve`, …) refuse `E-KIND-GONE` and
//! point at `emath run`. Host entry is [`run`] -> [`CliExit`].
//! Exit codes: 0 completed, 2 admission, 3 unmet/partial, 4 fault,
//! 5 incompatible checkpoint.

#![forbid(unsafe_code)]

pub mod catalog;
mod capabilities;
mod diagnostics;
mod execution;
mod language_cmd;
mod provenance_cmd;
mod tooling_cmd;
mod triage;
mod pedagogy;
mod terminal;

mod cli_help;
mod cli_build;
mod cli_check;
mod cli_dispatch;
mod cli_json;
mod cli_parse;
mod loop_cmd;
mod dream_cmd;
mod keep_cmd;
mod export_cmd;
mod compare_cmd;
mod experiment_cmd;

pub(crate) use pedagogy::PedagogicError;

pub(crate) use cli_help::*;
pub(crate) use cli_build::*;
pub use cli_build::{run_check, run_check_source};
pub use cli_check::{admitted_meaning_id, check};
pub(crate) use cli_dispatch::*;
pub(crate) use cli_json::*;
pub use cli_json::admitted_tree;
pub use cli_parse::run;
pub(crate) use cli_parse::*;

use emath_core::Diagnostics;
use emath_sema::CompilerSession;
use std::path::{Path, PathBuf};

/// Host process exit codes; `EXIT_MEANINGS` below says what each means.
/// The variant names are historical: use the `EXIT_*` constants, whose
/// names carry the current meaning (`EXIT_PARTIAL` is 3, not a toolchain
/// error).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum CliExit {
    Ok = 0,
    Refused = 1,
    Usage = 2,
    Toolchain = 3,
    Io = 4,
    Safety = 5,
}

pub const EXIT_OK: CliExit = CliExit::Ok;
pub const EXIT_REFUSED: CliExit = CliExit::Refused;
pub const EXIT_USAGE: CliExit = CliExit::Usage;
pub const EXIT_TOOLCHAIN: CliExit = CliExit::Toolchain;
pub const EXIT_IO: CliExit = CliExit::Io;
pub const EXIT_SAFETY: CliExit = CliExit::Safety;
pub const EXIT_ADMISSION: CliExit = CliExit::Usage;
pub const EXIT_PARTIAL: CliExit = CliExit::Toolchain;
pub const EXIT_FAULT: CliExit = CliExit::Io;
pub const EXIT_CHECKPOINT: CliExit = CliExit::Safety;

/// What each exit code means, for help and `capabilities --json`.
pub const EXIT_MEANINGS: [(CliExit, &str); 6] = [
    (EXIT_OK, "ok: the command completed"),
    (EXIT_REFUSED, "rejected: a test or checker rejected a claim (`test`, `export lean --check`, `fmt`), or a command refused a well-formed request (for example resuming a saved run whose source changed)"),
    (EXIT_ADMISSION, "admission: syntax, type, input, or usage error"),
    (EXIT_PARTIAL, "partial: unmet, suspended, out of work, not translated, not proven, or a tool is missing"),
    (EXIT_FAULT, "fault: execution fault or named refusal, or a file could not be read or written"),
    (EXIT_CHECKPOINT, "checkpoint: incompatible or corrupt checkpoint"),
];

pub fn exit_from_diagnostics(has_errors: bool) -> CliExit {
    if has_errors { EXIT_ADMISSION } else { EXIT_OK }
}

pub use provenance_cmd::provenance_explanation;
