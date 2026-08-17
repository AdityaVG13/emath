//! Bootstrap syntax crate: layout lexer and recursive-descent parser.
//!
//! The parser is replaceable by the Phase 4 lossless parser but already
//! guarantees: no panics on arbitrary UTF-8, exact spans on every token and
//! node, indentation enforcement, duplicate-section checks, precedence,
//! bounded source/token/nesting limits, and recovery at statement
//! boundaries. This crate is provider-free.
//!
//! Semantic Genesis: [`genesis`] parses `emath custom` world declarations
//! (G0). The G1 world/forest stage (bounded parse forest + signature
//! inference over `emath-term`/`emath-world-ir` values) lives in
//! `emath-genesis` since the world-side fence; the CLI consumes it
//! directly (`emath_genesis::forest`), so this crate carries no
//! emath-genesis dependency.
//!
//! The syntax tree is owned by `emath-core` (`emath_core::tree`); this crate
//! re-exports it and implements the kernel [`emath_core::parse::SourceParser`]
//! seam so that `emath-sema` can admit without depending on this crate.

#![forbid(unsafe_code)]

pub mod formatter;
pub mod genesis;
pub mod lexer;
pub mod parser;
pub mod token;
pub mod tree;

use emath_core::{Diagnostics, FileId, limits::Limits};
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

/// The `.emath` parser as a kernel [`emath_core::parse::SourceParser`]
/// implementation, injected into `emath-sema` sessions at runtime.
#[derive(Clone, Copy, Debug, Default)]
pub struct SyntaxParser;

impl emath_core::parse::SourceParser for SyntaxParser {
    fn parse(&self, text: &str, file: FileId, limits: &Limits) -> (SyntaxTree, Diagnostics) {
        parser::parse(text, file, limits)
    }
}

/// Install [`SyntaxParser`] as the process-wide default source parser.
//
// Hosts that construct `CompilerSession` values which parse must call this
// once per process before their first parse (the CLI and LSP do so at
// startup). Idempotent.
pub fn install_source_parser() {
    static PARSER: std::sync::OnceLock<SyntaxParser> = std::sync::OnceLock::new();
    emath_core::parse::register_source_parser(PARSER.get_or_init(|| SyntaxParser));
}
