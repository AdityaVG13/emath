//! Structured LaTeX math frontend (SG-11).

use emath_genesis::{BinderDomain, BinderFamily, BinderKind, BinderTerm, ScopedBinder};
use emath_term::{SymbolId, Term, VariableId};

use crate::layout::graph::{
    GraphBuilder, LayoutContent, LayoutError, MathLayoutGraph, NodeId, SpatialRelation,
};

const KNOWN_COMMANDS: &[&str] = &["frac", "sqrt", "sum", "prod", "int", "lim", "to"];

const GREEK: &[&str] = &[
    "alpha",
    "beta",
    "gamma",
    "delta",
    "epsilon",
    "zeta",
    "eta",
    "theta",
    "iota",
    "kappa",
    "lambda",
    "mu",
    "nu",
    "xi",
    "omicron",
    "pi",
    "rho",
    "sigma",
    "tau",
    "upsilon",
    "phi",
    "chi",
    "psi",
    "omega",
    "Gamma",
    "Delta",
    "Theta",
    "Lambda",
    "Xi",
    "Pi",
    "Sigma",
    "Upsilon",
    "Phi",
    "Psi",
    "Omega",
    "varepsilon",
    "vartheta",
    "varpi",
    "varrho",
    "varsigma",
    "varphi",
];

/// Import structured LaTeX (or a mixed document with `$...$` / `\[...\]`)
/// into a layout graph, preserving the original source byte-exactly.
pub fn parse_latex(source: &str) -> Result<MathLayoutGraph, LayoutError> {
    if has_formula_delimiters(source) {
        parse_document(source)
    } else {
        parse_bare_math(source)
    }
}

/// Lower a layout graph to a binder term. Extraction never fabricates a
/// term: structured subset only, otherwise [`LayoutError::Unlowered`].
pub fn to_binder_term(graph: &MathLayoutGraph) -> Result<BinderTerm, LayoutError> {
    let root = graph
        .formula_regions()
        .next()
        .map(|node| node.id)
        .or_else(|| graph.nodes().first().map(|node| node.id))
        .ok_or_else(|| LayoutError::Unlowered {
            reason: "empty layout graph".to_string(),
        })?;
    lower_id(graph, root)
}

mod ast;
mod climb;
mod emit;
mod env;
mod lower;
mod parser;

use ast::*;
use climb::*;
use emit::*;
use env::*;
use lower::*;
use parser::*;
