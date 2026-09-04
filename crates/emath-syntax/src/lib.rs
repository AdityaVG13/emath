//! Bootstrap syntax crate: layout lexer and recursive-descent parser.
//!
//! Panic-free on arbitrary UTF-8, exact spans everywhere, bounded
//! source/token/nesting limits, recovery at statement boundaries.
//! The syntax tree is owned by `emath-core`; this crate re-exports it and
//! implements the kernel [`emath_core::parse::SourceParser`] seam.
//! Meaning-budget surfaces (`expand_scratch`, `apply_solve_candidate`,
//! `exactness_ledger`) are crate-root re-exports from `scratch` / `exactness`.

#![forbid(unsafe_code)]

pub mod edition;
pub mod exactness;
pub mod formatter;
pub mod genesis;
pub mod layout;
pub mod lexer;
pub mod parser;
pub mod scratch;
pub mod stage0;
pub mod token;
pub mod tree;

pub use edition::{GrammarProfile, admitted_by_default, grammar_profile_for};
pub use layout::{E_SYN_HANGING_INFIX, classify_line_break};
// Facade fence (C053): the formatter entry point callers consume
// deep (`formatter::format`) is root-exported; the module stays public
// for format_type and friends.
pub use exactness::{
    ExactnessDimension, ExactnessEntry, ExactnessLedger, ExactnessStatus, exactness_ledger,
    exactness_ledger_raised, explanation_notes,
};
pub use formatter::format;
pub use scratch::{
    ExpansionOutcome, HoleCandidate, HoleContinuation, HoleKind, HoleRecord, HoleRejection,
    ScratchExpansion, ScratchLevel, ScratchNote, ScratchRewriteLevel, SolveIntent, SolveWorld,
    apply_solve_candidate, expand_scratch,
};
pub use stage0::{
    EXCLUDED_DOMAIN_FORMS, PreservedGlyph, STAGE0_FORMS, forbidden_domain_matches, unknown_glyphs,
};

use emath_core::{Diagnostics, Edition, FileId, limits::Limits};
use token::Comment;
use tree::SyntaxTree;

/// Parse an in-memory source into a syntax tree.
///
/// L0/L1 scratch and L2 named shorthand are expanded first so every host
/// (CLI, WASM, LSP) sees the same contracted declaration IR. Inspect the
/// expansion with [`expand_scratch`] / `emath expand`.
#[must_use]
pub fn parse(text: &str, file: FileId, limits: &Limits) -> (SyntaxTree, Diagnostics) {
    parse_with_edition(text, file, limits, Edition::Ed2026)
}

/// Parse using the grammar and deprecation policy selected by a package
/// edition. Historical editions remain available for replay.
#[must_use]
pub fn parse_with_edition(
    text: &str,
    file: FileId,
    limits: &Limits,
    edition: Edition,
) -> (SyntaxTree, Diagnostics) {
    // Same ceiling the lexer enforces: do not wrap/rewrite a source that
    // will be refused as E-SYN-116 anyway.
    if limits.check_source(text.len()).is_err() {
        return parser::parse(text, file, limits);
    }
    let expansion = expand_scratch(text);
    let source = expansion.parse_source(text);
    let (tree, mut diagnostics) = parser::parse(source, file, limits);
    diagnostics.extend_from(&expansion.diagnostics);
    apply_edition_policy(text, file, &mut diagnostics, edition);
    (tree, diagnostics)
}

fn apply_edition_policy(text: &str, file: FileId, diagnostics: &mut Diagnostics, edition: Edition) {
    let mut line_start = 0_u32;
    for line in text.lines() {
        if !is_deprecated_example_assignment(line) {
            line_start += line.len() as u32 + 1;
            continue;
        }
        let source = emath_core::Span::new(file, line_start, line_start);
        match edition {
            Edition::Ed2026 => diagnostics.warning(
                "W-EDITION-DEPRECATED",
                "top-level `example name = value` is deprecated in edition 2026; migrate it into a named `tests:` example",
                source,
            ),
            Edition::Ed2030 => diagnostics.error(
                "E-EDITION-HIDDEN",
                "top-level `example name = value` is hidden in edition 2030; replay with edition 2026 or migrate it into `tests:`",
                source,
            ),
        }
        line_start += line.len() as u32 + 1;
    }
}

/// Parse a source with default limits.
#[must_use]
pub fn parse_str(text: &str) -> (SyntaxTree, Diagnostics) {
    parse(text, FileId(0), &Limits::default())
}

/// True only for the historical top-level `example name = value` assignment
/// form. The current named form (`example <name>:` inside a `tests:` block)
/// must not be flagged: the deprecation policy targets the assignment
/// syntax, not the `example` keyword.
fn is_deprecated_example_assignment(line: &str) -> bool {
    let Some(rest) = line.trim_start().strip_prefix("example ") else {
        return false;
    };
    let rest = rest.trim_start();
    // Named examples open with `<name>:`; anything angle-bracketed is the
    // current form.
    if rest.starts_with('<') {
        return false;
    }
    // Deprecated form is `identifier = value`: an identifier, optional
    // whitespace, then `=` before any `:` or end of the assignment.
    let Some(eq) = rest.find('=') else {
        return false;
    };
    let name = rest[..eq].trim_end();
    !name.is_empty() && name.chars().all(|c| c == '_' || c.is_alphanumeric())
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
    if limits.check_source(text.len()).is_err() {
        let (tree, diagnostics) = parser::parse(text, file, limits);
        return LosslessParse {
            tree,
            diagnostics,
            comments: Vec::new(),
        };
    }
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
    fn parse(
        &self,
        text: &str,
        file: FileId,
        limits: &Limits,
        edition: Edition,
    ) -> (SyntaxTree, Diagnostics) {
        parse_with_edition(text, file, limits, edition)
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
