//! Semantic Genesis CLI: `parse`, `signature`, `genesis`, `compile
//! --parametric`, `world show`, `portfolio show`.
//!
//! Pipeline: source bytes → glyphs → parse forest → signature inference →
//! Term IR → free world → world candidates → interpretation portfolio →
//! answer receipt → parametric Rust artifact. Emitted JSON is deterministic
//! and std-only.

use super::{CliExit, CompileRequest, EXIT_OK, EXIT_REFUSED, EXIT_USAGE};
use crate::portfolio::{
    Authority, CollapsePolicy, InterpretationCandidate, InterpretationPolicy,
    InterpretationPortfolio, MetricAxis, MetricPolarity, PROVENANCE_USER_LOCKED, PortfolioError,
    ScoreVector, apply_portfolio_cap, evaluate,
};
use emath_core::limits::Limits;
use emath_genesis::{
    BooleanAlienWorld, CSA_MEANING_CLAIM, CSA_SCHEMA, CSA_SCHEMA_VERSION, Environment,
    FreeTermWorld, ModularAlienWorld, OnePointWorld, SeededCsaWorld, VM_SCHEMA, VM_SCHEMA_VERSION,
    VmBudget, VmOutcome, forest, free_symbolic_world, run as vm_run,
};
use emath_syntax::genesis as genesis_syntax;
use emath_term::{Signature, TERM_IR_VERSION, Term, VariableId};
use emath_world_ir::{
    Fixity, MeaningOrigin, OperatorDef, OperatorSemantics, SymbolDef, WorldIr, fnv1a64,
};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Component, Path, PathBuf};

mod analysis;
mod answer;
mod commands;
mod compile;
mod genesis;
mod worlds;

pub use analysis::*;
pub use answer::*;
pub use commands::*;
pub use compile::*;
pub use genesis::*;
pub use worlds::*;
