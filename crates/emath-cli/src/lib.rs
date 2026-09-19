//! Production `emath` CLI (constructor layer): `check`, `run`, `step`,
//! `inspect`, `verify`, `test`, `build`, `api`. Extracted tokens
//! (`eval`, `sweep`, `genesis`, `solve`, …) refuse `E-KIND-GONE` and
//! point at `emath run`. Host entry is [`run`] -> [`CliExit`].
//! Exit codes: 0 completed, 2 admission, 3 unmet/partial, 4 fault,
//! 5 incompatible checkpoint.

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
pub mod terminal;

mod cli_artifacts;
mod cli_build;
mod cli_check;
mod cli_dispatch;
mod cli_json;
mod cli_parse;
mod compiled_search;
mod project_lock;

pub use pedagogy::PedagogicError;
pub use terminal::{
    color_mode, is_ci, is_interactive, set_color_mode, set_no_color, should_color_stderr,
    should_color_stdout,
};

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

use emath_core::Diagnostics;
use emath_plan::{
    PlanInspection, PlannerConfig, PlanningOutcome, emit_provider_trait, lift_missing,
    plan as run_planner,
};
use emath_provider_api::{ProviderRegistry, RegistryConfig};
use emath_sema::CompilerSession;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Host process exit code mapping (the constitution):
/// - 0: command completed its declared operation
/// - 2: syntax/type/input admission failure
/// - 3: executed with an unmet, partial, or suspended answer
/// - 4: execution/backend fault
/// - 5: incompatible or corrupt checkpoint
///
/// Legacy names remain so existing call sites compile. Constructor-layer
/// commands must use the constitution numbers, not historical "refused=1".
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

pub fn exit_from_diagnostics(has_errors: bool) -> CliExit {
    if has_errors { EXIT_ADMISSION } else { EXIT_OK }
}

pub use provenance_cmd::provenance_explanation;

pub mod lsp;


pub mod portfolio;
