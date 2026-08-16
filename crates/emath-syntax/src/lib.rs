//! Bootstrap syntax crate: layout lexer and recursive-descent parser.
//!
//! The parser is replaceable by the Phase 4 lossless parser but already
//! guarantees: no panics on arbitrary UTF-8, exact spans on every token and
//! node, indentation enforcement, duplicate-section checks, precedence,
//! bounded source/token/nesting limits, and recovery at statement
//! boundaries. This crate is provider-free.
//!
//! Semantic Genesis adds two modules: [`genesis`] parses `emath custom`
//! world declarations (G0) and [`forest`] builds the bounded parse forest and
//! infers the world signature (G1).

#![forbid(unsafe_code)]

pub mod forest;
pub mod formatter;
pub mod genesis;
pub mod lexer;
pub mod parser;
pub mod token;
pub mod tree;

use emath_core::{limits::Limits, Diagnostics, FileId};
use token::Comment;
use tree::SyntaxTree;

/// Parse an in-memory source into a syntax tree.
#[must_use]
pub fn parse(text: &str, file: FileId, limits: &Limits) -> (SyntaxTree, Diagnostics) {
    parser::parse(text, file, limits)
}

/// Parse a source with default limits.
#[must_use]
pub fn parse_str(text: &str) -> (SyntaxTree, Diagnostics) {
    parser::parse(text, FileId(0), &Limits::default())
}

/// The lossless parse result (/002): the tree plus every
/// retained comment with its span, for formatting round-trips.
#[derive(Clone, Debug)]
pub struct LosslessParse {
    pub tree: SyntaxTree,
    pub diagnostics: Diagnostics,
    pub comments: Vec<Comment>,
}

/// Parse with comment retention. Deterministic: tokenization and parsing
/// are pure over the source bytes.
#[must_use]
pub fn parse_lossless(text: &str, file: FileId, limits: &Limits) -> LosslessParse {
    let (_, _, comments) = lexer::lex_with_comments(text, file, limits);
    let (tree, diagnostics) = parser::parse(text, file, limits);
    LosslessParse {
        tree,
        diagnostics,
        comments,
    }
}

/// Format the lossless parse canonically: idempotent, comment-preserving,
/// parse-stable .
#[must_use]
pub fn format_lossless(parse: &LosslessParse) -> String {
    formatter::format(&parse.tree, &parse.comments)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::{ExprKind, Item, StmtKind};

    const AFFINE_POLICY: &str = include_str!("../../../implementation/tests/valid/stateful.emath");
    const MINIMAL: &str = include_str!("../../../implementation/tests/valid/minimal.emath");
    const HELLO_SQUARE: &str = include_str!("../../../language/examples/00_hello_square.emath");
    const TEMPLATE_MINIMAL: &str = include_str!("../../../language/templates/minimal.emath");

    fn parse_clean(text: &str) -> SyntaxTree {
        let (tree, diagnostics) = parse_str(text);
        assert!(
            diagnostics.is_empty(),
            "expected clean parse, got:\n{}",
            diagnostics
                .items()
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n")
        );
        tree
    }

    #[test]
    fn valid_fixtures_parse_cleanly() {
        for source in [AFFINE_POLICY, MINIMAL, HELLO_SQUARE, TEMPLATE_MINIMAL] {
            parse_clean(source);
        }
    }

    #[test]
    fn affine_policy_declaration_shape() {
        let tree = parse_clean(AFFINE_POLICY);
        let Item::Declaration(decl) = &tree.items[0] else {
            panic!("expected a declaration");
        };
        assert_eq!(decl.name, "AffinePolicy");
        assert_eq!(decl.as_kind, "policy");
        let names: Vec<&str> = decl.sections().map(|s| s.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "inputs",
                "outputs",
                "state",
                "constructors",
                "definitions",
                "requests",
                "exports",
                "compile"
            ]
        );
        let definitions = decl.sections().find(|s| s.name == "definitions").unwrap();
        match &definitions.suite.statements[0].kind {
            StmtKind::Assign { target, value } => {
                assert_eq!(target.segments, ["score"]);
                assert!(matches!(value.kind, ExprKind::Binary { .. }));
            }
            other => panic!("unexpected definition statement: {other:?}"),
        }
    }

    #[test]
    fn template_minimal_parses_with_real_type() {
        let tree = parse_clean(TEMPLATE_MINIMAL);
        let Item::Declaration(decl) = &tree.items[0] else {
            panic!()
        };
        let inputs = decl.sections().find(|s| s.name == "inputs").unwrap();
        let StmtKind::FieldDecl { name, .. } = &inputs.suite.statements[0].kind else {
            panic!()
        };
        assert_eq!(name, "x");
    }

    #[test]
    fn invalid_fixture_style_comments_do_not_break_parsing() {
        // `# expect: E-NAME ...` first-line comments are fixture conventions.
        let (_, diagnostics) = parse_str("# expect: E-TYPE-001 unknown type\nemath custom <T> as function:\n    inputs:\n        x: RealText\n");
        // The comment must not produce a diagnostic; the parse continues.
        assert!(!diagnostics
            .items()
            .iter()
            .any(|d| d.code == "E-SYN-101" && d.message.contains('#')));
    }
}
