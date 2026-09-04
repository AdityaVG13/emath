//! In-memory emath compiler pipeline for the web demo.
//!
//! Safe dispatch lives here and is unit-testable on the host. The tiny C
//! ABI (`em_alloc` / `em_free` / `em_run` / `em_init`) is confined to [`ffi`].

#![deny(unsafe_code)]
#![deny(missing_docs)]

/// C ABI leaf: `em_alloc`, `em_free`, `em_run`, `em_init`.
#[allow(unsafe_code)]
pub mod ffi;

pub use ffi::install_panic_hook;

pub mod desugar;

pub use desugar::prepare_source;

use emath_artifact::{JsonValue, JsonWriter, parse_json_document};
use emath_core::{Diagnostics, FileId, Severity, limits::Limits};
use emath_exec_ir::interp::{Value, format_f64};
use emath_exec_ir::runner::{DeclarationRun, RunReport, TestRun, run_package_with_given};
use emath_genesis::{Disposition, ResultBundle, WorldResult};
use emath_ir::Mig;
use emath_rust_backend::BackendInput;
use emath_sema::session::CompilerSession;
use emath_syntax::{format_lossless, install_source_parser, parse_lossless};
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::sync::OnceLock;

mod examples;
mod ops;
mod payload;
mod serialize;
mod solve;

pub use examples::*;
use ops::*;
pub use payload::*;
use serialize::*;
use solve::*;

use examples::*;
use payload::*;
