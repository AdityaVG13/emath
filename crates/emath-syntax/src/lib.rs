//! Bootstrap syntax crate: layout lexer and recursive-descent parser.
//!
//! Panic-free on arbitrary UTF-8, exact spans everywhere, bounded
//! source/token/nesting limits, recovery at statement boundaries.
//! The syntax tree is owned by `emath-core`; this crate re-exports it and
//! implements the kernel [`emath_core::parse::SourceParser`] seam.

#![forbid(unsafe_code)]

pub mod exactness;
pub mod formatter;
pub mod genesis;
pub mod lexer;
pub mod parser;
pub mod scratch;
pub mod token;
pub mod tree;

pub use exactness::{
    ExactnessDimension, ExactnessEntry, ExactnessLedger, ExactnessStatus, exactness_ledger,
    exactness_ledger_raised, explanation_notes,
};
pub use scratch::{
    HoleCandidate, HoleContinuation, HoleRecord, HoleRejection, ScratchExpansion, ScratchLevel,
    ScratchNote, SolveCandidate, apply_solve_candidate, expand_scratch,
};

use emath_core::{Diagnostics, FileId, limits::Limits};
use token::Comment;
use tree::SyntaxTree;

/// Parse an in-memory source into a syntax tree.
///
/// L0/L1 scratch and L2 named shorthand are expanded first so every host
/// (CLI, WASM, LSP) sees the same contracted declaration IR. Inspect the
/// expansion with [`expand_scratch`] / `emath expand`.
#[must_use]
pub fn parse(text: &str, file: FileId, limits: &Limits) -> (SyntaxTree, Diagnostics) {
    let expansion = expand_scratch(text);
    let source = expansion.parse_source(text);
    let (tree, mut diagnostics) = parser::parse(source, file, limits);
    diagnostics.extend_from(&expansion.diagnostics);
    (tree, diagnostics)
}

/// Parse a source with default limits.
#[must_use]
pub fn parse_str(text: &str) -> (SyntaxTree, Diagnostics) {
    parse(text, FileId(0), &Limits::default())
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
    let expansion = expand_scratch(text);
    let source = expansion.parse_source(text);
    let (_, _, comments) = lexer::lex_with_comments(source, file, limits);
    let (tree, mut diagnostics) = parser::parse(source, file, limits);
    diagnostics.extend_from(&expansion.diagnostics);
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
        parse(text, file, limits)
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
