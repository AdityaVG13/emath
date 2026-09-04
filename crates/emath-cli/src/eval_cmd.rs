//! `emath eval` / `emath repl`: receipt-carrying evaluation.
//!
//! Two lanes, discriminated by source format and flags:
//! - Genesis-format reference files (`world …:` headers) evaluate on the
//!   semantic VM through `genesis_cmd::analyze` + `emath_genesis::run`
//!   under `--world` (default `free_symbolic`).
//! - Standard function-spec `.emath` files execute an admitted `emath
//!   function` declaration through the GENERIC stack — sema admission,
//!   `definition_order` / `lower_definition` EMIR lowering, reference-VM
//!   evaluation — and return a deterministic `emath.eval-function`
//!   receipt (or a typed E-EVAL-* refusal). No genesis-only fallback, no
//!   second evaluator, no domain branch.

use super::genesis_cmd::{self, Analysis};
use super::world_ir_eval::WorldIrValue;
use super::{
    CliExit, EXIT_OK, EXIT_REFUSED, json_diagnostic_entry, json_diagnostics_entries,
    print_diagnostics, print_json_diagnostics, split_error_code,
};
use emath_artifact::JsonWriter;
use emath_core::limits::Limits;
use emath_exec_ir::interp::Value;
use emath_exec_ir::runner::{TestVerdict, eval_definitions_values, run_declaration};
use emath_genesis::{
    BooleanAlienWorld, Environment, FreeTermWorld, ModularAlienWorld, OnePointWorld,
    SeededCsaWorld, VmBudget, VmOutcome, run as vm_run,
};
use emath_ir::TypeNode;
use emath_sema::CompilerSession;
use emath_term::{Term, VariableId};
use emath_world_ir::WorldIr;
use std::collections::BTreeMap;
use std::io::{self, BufRead};
use std::path::{Path, PathBuf};

mod args;
mod eval;
mod repl;
mod spec;
mod sweep;

pub(crate) use args::*;
pub(crate) use eval::*;
pub(crate) use repl::*;
pub(crate) use spec::*;
pub(crate) use sweep::*;
