use crate::rust_ir::ast::{Block, Expr, FnDef, Item, Param, Stmt, Ty, Visibility, escape_ident};
use emath_exec_ir::{definition_order, lower_definition};
use emath_ir::{Extent, SemanticPackage, TypeNode};
use std::collections::BTreeSet;

use crate::BackendError;
use crate::codegen_helpers::{
    add_obligations, collect_var_names, expand_host_inputs, field_value_kinds,
};
use crate::codegen_render::value_expr_rate;

mod newton_impl;
mod steps;
