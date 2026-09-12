//! Historical `emath-lab` host. Combined [`run`] forwards constructor
//! `emath` tokens and refuses extracted lab tokens (`E-KIND-GONE`).
//! Command bodies stay on disk; they are not a second language.

#![forbid(unsafe_code)]

pub use emath_cli::{
    CliExit, EXIT_OK, EXIT_REFUSED, EXIT_USAGE, admitted_meaning_id, assign_once,
    exit_from_diagnostics, json_diagnostic_entry, json_diagnostics_entries, json_put_opt,
    print_diagnostics, print_json_diagnostics, print_missing_newline, read_emath_source,
    goal_json_rows, plan_json_document, refuse_coded, refuse_unverified_language_image, run_check,
    split_error_code, take_nonflag_value, usage,
};

use emath_core::Diagnostics;
use emath_syntax::ExactnessStatus;
use std::path::{Path, PathBuf};

pub mod agent_protocol;
pub mod catalog;
pub mod coverage_cmd;
pub mod coverage_seed;
pub mod eval_cmd;
pub mod serve_cmd;
pub mod world_ir_eval;
pub mod genesis_cmd;
pub mod layout;

mod agent_cmd;
mod cli_freeze;
mod cli_scratch;
mod dispatch;
mod fit_cmd;
mod host_cmd;
mod provider_cmd;
mod vendor_cmd;
mod library_cmd;
pub mod meaning_cmd;

pub use cli_scratch::*;
pub use dispatch::run;
pub use host_cmd::{architecture, architecture_json, artifact_battery, import_modelica_cmd};

pub(crate) use cli_freeze::*;
pub(crate) use emath_cli::{
    CompileRequest, FileJsonRequest, GenesisRequest, ParseRequest, SignatureRequest,
    parse_compile_request, parse_file_json_request, parse_genesis_request, parse_parse_request,
    parse_show_named, parse_signature_request,
};
