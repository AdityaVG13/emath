//! Indentation-aware lexer with bounded resource consumption.
//!
//! Emits `Newline`/`Indent`/`Dedent` layout tokens. Newlines are suppressed
//! inside `()`, `[]`, and `{}` so multi-line argument lists lex as one flow.
//! Comments: `#` (ordinary) and `///` (documentation) to end of line.
//! `//` is the exact-rational separator (`3//7`), not a comment.

use crate::token::{Comment, Keyword, NablaForm, Token, TokenKind};
use emath_core::{Diagnostics, FileId, Span, limits::Limits};

/// Lex the whole source into layout-aware tokens (comments skipped).
#[must_use]
pub fn lex(source: &str, file: FileId, limits: &Limits) -> (Vec<Token>, Diagnostics) {
    let mut diagnostics = Diagnostics::new();
    // Enforce the byte ceiling before scanning so huge inputs cannot burn
    // O(n) work after a session/host forgot `Limits::check_source`.
    if let Err(max) = limits.check_source(source.len()) {
        diagnostics.error(
            "E-SYN-116",
            format!("source is {} bytes; limit is {max} bytes", source.len()),
            Span::new(file, 0, 0),
        );
        let end = u32::try_from(source.len()).unwrap_or(u32::MAX);
        return (
            vec![Token {
                kind: TokenKind::Eof,
                span: Span::new(file, end, end),
            }],
            diagnostics,
        );
    }
    let mut lexer = Lexer {
        source,
        file,
        limits,
        bytes: source.as_bytes(),
        pos: 0,
        tokens: Vec::new(),
        diagnostics,
        indent_stack: vec![0],
        paren_depth: 0,
        nesting: 0,
        comments: Vec::new(),
        keep_comments: false,
    };
    lexer.lex_lines();
    let end = u32::try_from(source.len()).unwrap_or(u32::MAX);
    lexer.tokens.push(Token {
        kind: TokenKind::Eof,
        span: Span::new(file, end, end),
    });
    (lexer.tokens, lexer.diagnostics)
}

/// Lex the whole source into layout-aware tokens and retain every comment
/// with its span (lossless tokenization / canonical formatting).
#[must_use]
pub fn lex_with_comments(
    source: &str,
    file: FileId,
    limits: &Limits,
) -> (Vec<Token>, Diagnostics, Vec<Comment>) {
    let mut diagnostics = Diagnostics::new();
    if let Err(max) = limits.check_source(source.len()) {
        diagnostics.error(
            "E-SYN-116",
            format!("source is {} bytes; limit is {max} bytes", source.len()),
            Span::new(file, 0, 0),
        );
        let end = u32::try_from(source.len()).unwrap_or(u32::MAX);
        return (
            vec![Token {
                kind: TokenKind::Eof,
                span: Span::new(file, end, end),
            }],
            diagnostics,
            Vec::new(),
        );
    }
    let mut lexer = Lexer {
        source,
        file,
        limits,
        bytes: source.as_bytes(),
        pos: 0,
        tokens: Vec::new(),
        diagnostics,
        indent_stack: vec![0],
        paren_depth: 0,
        nesting: 0,
        comments: Vec::new(),
        keep_comments: true,
    };
    lexer.lex_lines();
    let end = u32::try_from(source.len()).unwrap_or(u32::MAX);
    lexer.tokens.push(Token {
        kind: TokenKind::Eof,
        span: Span::new(file, end, end),
    });
    (lexer.tokens, lexer.diagnostics, lexer.comments)
}

struct Lexer<'a> {
    source: &'a str,
    file: FileId,
    limits: &'a Limits,
    bytes: &'a [u8],
    pos: usize,
    tokens: Vec<Token>,
    diagnostics: Diagnostics,
    indent_stack: Vec<usize>,
    paren_depth: u32,
    nesting: usize,
    /// Retained comments when `keep_comments` is set.
    comments: Vec<Comment>,
    keep_comments: bool,
}

mod engine;
