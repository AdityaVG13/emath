//! Tooling commands kept in production `emath`: `new`, `fmt`, `migrate`,
//! `explain`, `test`, artifact `verify` / `inspect`, `diff`, `doctor`.
//! Source execution commands live in `crate::execution`.

use std::path::{Path, PathBuf};
use std::process::Command;

use emath_artifact::JsonWriter;
use emath_core::content_id_of_str;
use emath_sema::CompilerSession;

use crate::{
    CliExit, EXIT_ADMISSION, EXIT_FAULT, EXIT_IO, EXIT_OK, EXIT_REFUSED, EXIT_SAFETY,
    EXIT_TOOLCHAIN, EXIT_USAGE, ExplainRequest, artifact_check, json_diagnostic_entry,
    print_json_diagnostics,
};

mod doctor;
mod explain;
mod fmt;
mod inspect;
mod migrate;
mod new;
mod run;

pub(crate) use doctor::*;
pub(crate) use explain::*;
pub(crate) use fmt::*;
pub(crate) use inspect::*;
pub(crate) use migrate::*;
pub(crate) use new::*;
pub(crate) use run::*;

