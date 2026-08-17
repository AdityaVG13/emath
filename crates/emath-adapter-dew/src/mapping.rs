//! Source mapping.
//!
//! Maintains the mapping from SIR/EMIR nodes through Dew expression
//! nodes to generated symbols/spans. Entries are deterministic and
//! ordered by the emath node id; provider diagnostics translate into
//! these entries while the original details are retained.

use emath_core::Span;
use emath_ir::{ExprId, ExprNode, SemanticPackage};

/// One source-map entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceMapEntry {
    /// Emath expression id (index into the SIR arena).
    pub sir_node: u32,
    /// Path in the Dew expression ("root", "root.left"...).
    pub dew_path: String,
    /// Generated symbol in the backend fragment.
    pub generated_symbol: String,
    /// Byte-range span of the source node.
    pub span: (u32, u32),
    /// Stable node kind token.
    pub kind: String,
}

/// Builds the deterministic source map for an expression subtree.
#[must_use]
pub fn build_source_map(package: &SemanticPackage, root: ExprId) -> Vec<SourceMapEntry> {
    let mut entries = Vec::new();
    walk(package, root, "root", "t0", &mut entries);
    entries.sort_by_key(|entry| (entry.sir_node, entry.dew_path.clone()));
    entries
}

fn walk(
    package: &SemanticPackage,
    id: ExprId,
    dew_path: &str,
    symbol: &str,
    entries: &mut Vec<SourceMapEntry>,
) {
    // A node id that is absent from the package cannot be mapped: it is
    // skipped instead of recorded as a success entry;
    // map_expression refuses the same situation with E-PROV-030.
    let Some(node) = package.expr(id) else {
        return;
    };
    let span: Span = package.expr_span(id);
    let kind = node_kind(package.expr(id));
    entries.push(SourceMapEntry {
        sir_node: u32::try_from(id.index()).unwrap_or(u32::MAX),
        dew_path: dew_path.to_string(),
        generated_symbol: symbol.to_string(),
        span: (span.start, span.end),
        kind,
    });
    match node {
        ExprNode::Unary { value, .. } => {
            walk(
                package,
                *value,
                &format!("{dew_path}.value"),
                symbol,
                entries,
            );
        }
        ExprNode::Binary { left, right, .. } => {
            walk(package, *left, &format!("{dew_path}.left"), symbol, entries);
            walk(
                package,
                *right,
                &format!("{dew_path}.right"),
                symbol,
                entries,
            );
        }
        ExprNode::If {
            condition,
            then_value,
            else_value,
        } => {
            walk(
                package,
                *condition,
                &format!("{dew_path}.condition"),
                symbol,
                entries,
            );
            walk(
                package,
                *then_value,
                &format!("{dew_path}.then"),
                symbol,
                entries,
            );
            walk(
                package,
                *else_value,
                &format!("{dew_path}.else"),
                symbol,
                entries,
            );
        }
        ExprNode::Call { arguments, .. } => {
            for (index, argument) in arguments.iter().enumerate() {
                walk(
                    package,
                    *argument,
                    &format!("{dew_path}.arg{index}"),
                    symbol,
                    entries,
                );
            }
        }
        _ => {}
    }
}

fn node_kind(node: Option<&ExprNode>) -> String {
    match node {
        Some(ExprNode::Literal(_)) => "literal".into(),
        Some(ExprNode::Variable(_)) => "variable".into(),
        Some(ExprNode::Call { .. }) => "call".into(),
        Some(ExprNode::Unary { .. }) => "unary".into(),
        Some(ExprNode::Binary { .. }) => "binary".into(),
        Some(ExprNode::If { .. }) => "if".into(),
        Some(ExprNode::Record { .. }) => "record".into(),
        Some(ExprNode::Index { .. }) => "index".into(),
        Some(ExprNode::Binder { .. }) => "binder".into(),
        None => "missing".into(),
    }
}
