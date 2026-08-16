//!: source mapping.
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
    walk(package, root, "root", "v0", &mut entries);
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
    let span: Span = package.expr_span(id);
    let kind = node_kind(package.expr(id));
    entries.push(SourceMapEntry {
        sir_node: u32::try_from(id.index()).unwrap_or(u32::MAX),
        dew_path: dew_path.to_string(),
        generated_symbol: symbol.to_string(),
        span: (span.start, span.end),
        kind,
    });
    match package.expr(id) {
        Some(ExprNode::Unary { value, .. }) => {
            walk(
                package,
                *value,
                &format!("{dew_path}.value"),
                symbol,
                entries,
            );
        }
        Some(ExprNode::Binary { left, right, .. }) => {
            walk(package, *left, &format!("{dew_path}.left"), symbol, entries);
            walk(
                package,
                *right,
                &format!("{dew_path}.right"),
                symbol,
                entries,
            );
        }
        Some(ExprNode::If {
            condition,
            then_value,
            else_value,
        }) => {
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
        Some(ExprNode::Call { arguments, .. }) => {
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
        None => "unknown".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use emath_core::Span;
    use emath_ir::{BinaryOp, Literal};

    #[test]
    fn source_map_is_deterministic_and_anchored() {
        let mut package = SemanticPackage::new();
        let left = package.push_expr(ExprNode::Literal(Literal::FloatBits(0)), Span::default());
        let right = package.push_expr(
            ExprNode::Variable(emath_core::QualifiedName("x".into())),
            Span::default(),
        );
        let root = package.push_expr(
            ExprNode::Binary {
                operation: BinaryOp::StrictFloatMul,
                left,
                right,
            },
            Span::default(),
        );
        let first = build_source_map(&package, root);
        let again = build_source_map(&package, root);
        assert_eq!(first, again);
        assert!(first.iter().any(|entry| entry.kind == "binary"));
        assert!(first.iter().any(|entry| entry.kind == "literal"));
        assert!(first.iter().any(|entry| entry.kind == "variable"));
        assert!(first
            .windows(2)
            .all(|pair| pair[0].sir_node <= pair[1].sir_node));
    }
}
