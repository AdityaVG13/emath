use crate::rust_ir::ast::{
    BinOp, Block, Expr, FnDef, Item, Param, Stmt, Ty, Visibility, escape_ident,
};
use crate::rust_ir::render::render_expr;
use emath_exec_ir::{definition_order, lower_definition};
use emath_ir::{Extent, SemanticPackage, TypeNode};
use std::collections::BTreeSet;

use crate::BackendError;
use crate::codegen_helpers::{
    add_obligations, add_scaled_expr, collect_var_names, expand_host_inputs, i64_field_names,
    rate_call, rate_lets,
};
use crate::codegen_render::{value_expr, value_expr_rate};

/// Interpreter parity constants for generated causalized-Newton steps
/// (`crates/emath-exec-ir/src/runner/simulate/newton.rs`). Changing any
/// of these here without changing the interpreter breaks the parity claim
/// behind the admission message "implicit residual system did not
/// converge".
const NEWTON_MAX_ITER: usize = 30;
const NEWTON_TOL: f64 = 1e-9;
const NEWTON_FINAL_TOL: f64 = 1e-6;

mod newton_fns;
mod newton_impl;
mod steps;

use newton_fns::*;
pub(crate) use steps::*;
