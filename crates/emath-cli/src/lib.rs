//! emath CLI: `check`, `plan`, `build`, `artifact`, `architecture`, `web`, `serve`,
//! Semantic Genesis (`parse`, `expand`, `signature`, `genesis`, `eval`,
//! `repl`, `compile --parametric`, `world show`, `portfolio show`, `meaning`),
//! and meaning-budget (`solve`, `exactness`, `freeze`, `why`, `assumptions`).
//! Host entry is [`run`] -> [`CliExit`] (not a raw `u8`). Exit codes: 0 ok, 1 refused, 2 usage/io.

#![forbid(unsafe_code)]

mod agent_cmd;
pub mod catalog;
pub mod coverage_cmd;
pub mod coverage_seed;
pub mod diagnostics;
mod eval_cmd;
mod fit_cmd;
pub mod genesis_cmd;
pub mod language_cmd;
mod library_cmd;
pub mod meaning_cmd;
mod provenance_cmd;
pub mod serve_cmd;
pub mod simulate_cmd;
mod tooling_cmd;
mod world_ir_eval;

mod cli_artifacts;
mod cli_build;
mod cli_check;
mod cli_dispatch;
mod cli_freeze;
mod cli_json;
mod cli_parse;
mod cli_scratch;

pub use cli_artifacts::*;
pub use cli_build::*;
pub use cli_check::*;
pub(crate) use cli_dispatch::*;
pub(crate) use cli_freeze::*;
pub use cli_json::*;
pub use cli_parse::*;
pub use cli_scratch::*;

use emath_build::{BuildOptions, build_file};
use emath_core::Diagnostics;
use emath_plan::{
    PlanInspection, PlannerConfig, PlanningOutcome, emit_provider_trait, lift_missing,
    plan as run_planner,
};
use emath_provider_api::{ProviderRegistry, RegistryConfig};
use emath_sema::CompilerSession;
use emath_syntax::ExactnessStatus;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Closed 3-way host exit. `repr(u8)` is the process mapping (0/1/2), not a
/// public `u8` return; [`run`] returns `CliExit` and `main` matches exhaustively.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum CliExit {
    Ok = 0,
    Refused = 1,
    Usage = 2,
}

pub const EXIT_OK: CliExit = CliExit::Ok;
pub const EXIT_REFUSED: CliExit = CliExit::Refused;
pub const EXIT_USAGE: CliExit = CliExit::Usage;

fn exit_from_diagnostics(has_errors: bool) -> CliExit {
    if has_errors { EXIT_REFUSED } else { EXIT_OK }
}

pub use provenance_cmd::provenance_explanation;

pub mod lsp;

pub mod layout;

pub mod agent_protocol;

pub mod portfolio;
