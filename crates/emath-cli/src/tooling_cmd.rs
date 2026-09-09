//! Tooling commands kept in production `emath`: `new`, `fmt`, `migrate`,
//! `explain`, `test`, artifact `verify` / `inspect`, `diff`, `doctor`.
//! Source execution commands live in `crate::execution`.
//!
//! `vendor` / `provider` / `fork` / `bench` / `agent` live in `emath-cli-lab`.

use std::path::{Path, PathBuf};
use std::process::Command;

use emath_artifact::JsonWriter;
use emath_build::{BuildOptions, build_file};
use emath_core::content_id_of_str;
use emath_sema::CompilerSession;

use crate::{
    CliExit, EXIT_OK, EXIT_REFUSED, EXIT_USAGE, ExplainRequest, artifact_check, print_diagnostics,
};

mod doctor;
mod explain;
mod fmt;
mod inspect;
mod migrate;
mod new;
mod run;

pub use doctor::{DoctorProbe, doctor_probes};
pub(crate) use doctor::*;
pub(crate) use explain::*;
pub(crate) use fmt::*;
pub(crate) use inspect::*;
pub(crate) use migrate::*;
pub use new::{PROVIDERS, UPSTREAM_LOCK_REL, upstream_lock_path};
pub(crate) use new::*;
pub(crate) use run::*;

/// Maps a build error to the conventional exit class.
pub fn classify_build_error(error: &dyn std::fmt::Display) -> CliExit {
    let text = error.to_string();
    if text.contains("admission refused") {
        EXIT_REFUSED
    } else {
        EXIT_USAGE
    }
}
