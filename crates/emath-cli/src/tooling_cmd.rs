//! Tooling commands: `new`, `fmt`, `migrate`, `explain`, `run`, `test`,
//! `bench`, `verify`, `inspect`, `diff`, `doctor`, `vendor`, `provider`,
//! `fork`, and the structured `agent` API envelope.
//!
//! Implemented commands exercise the real pipeline (check/plan/build,
//! artifact verification). Capabilities outside the Phase 1 subset are
//! typed refusals with stable codes (`E-TLT-*`, `E-PROV-*`); nothing is
//! silently accepted.

use std::path::{Path, PathBuf};
use std::process::Command;

use emath_artifact::JsonWriter;
use emath_build::{BuildOptions, build_file, generated_crate_target_dir, run_cargo_timed};
use emath_core::content_id_of_str;
use emath_sema::CompilerSession;

use crate::{
    CliExit, EXIT_OK, EXIT_REFUSED, EXIT_USAGE, ExplainRequest, ForkRequest, ProviderRequest,
    artifact_check, print_diagnostics,
};

mod doctor;
mod explain;
mod fmt;
mod inspect;
mod migrate;
mod new;
mod provider;
mod run;
mod vendor;

pub(crate) use doctor::*;
pub(crate) use explain::*;
pub(crate) use fmt::*;
pub(crate) use inspect::*;
pub(crate) use migrate::*;
pub(crate) use new::*;
pub(crate) use provider::*;
pub(crate) use run::*;
pub(crate) use vendor::*;
