//! Production `emath` CLI: compiler / user surface (`check`, `plan`, `build`,
//! `parse`, `compile`, `simulate`, `run`, `test`, …). Extracted lab/host
//! commands live in `emath-cli-lab` (`emath-lab`).
//! Host entry is [`run`] -> [`CliExit`] (not a raw `u8`). Exit codes: 0 ok, 1 refused, 2 usage/io.

#![forbid(unsafe_code)]

pub mod catalog;
pub mod capabilities;
pub mod diagnostics;
pub mod execution;
pub mod language_cmd;
mod provenance_cmd;
pub mod simulate_cmd;
pub mod tooling_cmd;
pub mod triage;
pub mod pedagogy;

mod cli_artifacts;
mod cli_build;
mod cli_check;
mod cli_dispatch;
mod cli_json;
mod cli_parse;
mod compiled_search;
mod project_lock;

pub use pedagogy::PedagogicError;

pub use cli_artifacts::*;
pub use cli_build::*;
pub use cli_check::*;
pub(crate) use cli_dispatch::*;
pub use cli_dispatch::{
    CompileRequest, FileJsonRequest, GenesisRequest, ParseRequest, SignatureRequest, assign_once,
    parse_compile_request, parse_file_json_request, parse_genesis_request, parse_parse_request,
    parse_show_named, parse_signature_request, refuse_unverified_language_image, take_nonflag_value,
    usage,
};
pub use project_lock::refuse_malformed_project_lock;
pub use cli_json::*;
pub use cli_parse::*;

use emath_build::{BuildOptions, build_file};
use emath_core::Diagnostics;
use emath_plan::{
    PlanInspection, PlannerConfig, PlanningOutcome, emit_provider_trait, lift_missing,
    plan as run_planner,
};
use emath_provider_api::{ProviderRegistry, RegistryConfig};
use emath_sema::CompilerSession;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Host process exit code mapping:
/// - 0: Success (Ok)
/// - 1: Refused (mathematical refusal, check failure, or verification error)
/// - 2: Usage (syntax error, unknown flag, missing required arguments)
/// - 3: Toolchain (environment or toolchain prerequisite missing / doctor failure)
/// - 4: Io (file not found, cannot read/write file, or disk I/O error)
/// - 5: Safety (destructive action rejected, safety boundary check failed)
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

pub fn exit_from_diagnostics(has_errors: bool) -> CliExit {
    if has_errors { EXIT_REFUSED } else { EXIT_OK }
}

pub use provenance_cmd::provenance_explanation;

pub mod lsp;


pub mod portfolio;
