//! Bootstrap recursive-descent parser for the `.emath` surface.
//!
//! Covers the full structural grammar (sections, generics, records,
//! commands, precedence expressions, binders, quantity literals, chained
//! comparisons, continuations). Never panics; recovers at statement
//! boundaries; spans everywhere.

use crate::lexer::lex;
use crate::token::{Keyword, Token, TokenKind};
use crate::tree::{
    BinaryOp, BinderKind, Expr, ExprKind, Item, NotationFixity, StmtKind, Suite, SyntaxTree,
    Visibility,
};
use emath_core::{limits::Limits, Diagnostics, FileId, Span};
use std::collections::BTreeMap;

mod decl;
mod expr;
mod stmt;
mod stmt_binders;
mod stmt_idents;
mod stmt_suite;
mod types;

const MAX_EXPR_DEPTH: usize = 128;

/// Precedence floor of the custom-operator band. The core expression
/// ladder (Iff, Imply, Or, And, comparisons, `+ -`, `* /`, unary,
/// power, postfix) occupies tiers 1..=10; every `notation` declaration
/// carries an explicit precedence at or above [`CUSTOM_OP_MIN_PRECEDENCE`]
/// and custom infix operators bind tighter than `* /` and looser than
/// unary prefix (parenthesize `-(a ⊕ b)` to apply unary minus last).
pub(crate) const CUSTOM_OP_MIN_PRECEDENCE: u32 = 11;

/// N3 reserved glyphs: the core syntactic vocabulary cannot be rebound.
const NOTATION_RESERVED_GLYPHS: &[&str] = &[
    "+", "-", "*", "/", "^", "==", "!=", "<", "<=", ">", ">=", "and", "or", "not", "=", ":=",
    "->", "=>", "::", ".", "..", "..=", "?",
];

/// One file-scoped custom operator collected from a `notation` item.
#[derive(Clone, Debug)]
pub(crate) struct NotOp {
    pub fixity: NotationFixity,
    pub precedence: u32,
    pub target: Vec<String>,
}

/// Parse an in-memory source into a syntax tree.
#[must_use]
pub fn parse(source: &str, file: FileId, limits: &Limits) -> (SyntaxTree, Diagnostics) {
    let mut parser = Parser::new(source, file, limits);
    parser.pre_scan_notations();
    parser.parse_items();
    let mut diagnostics = parser.diagnostics;
    diagnostics.extend_from(&parser.lex_diagnostics);
    let tree = SyntaxTree {
        source: Span::new(file, 0, u32::try_from(source.len()).unwrap_or(u32::MAX)),
        items: parser.tree_items,
    };
    (tree, diagnostics)
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    diagnostics: Diagnostics,
    lex_diagnostics: Diagnostics,
    tree_items: Vec<Item>,
    /// File-scoped custom operators from `notation` items (including alias
    /// spellings), collected by [`Parser::pre_scan_notations`] before any
    /// expression parses (N1: scoped to the whole package/file).
    notations: BTreeMap<String, NotOp>,
    /// B02: when true, suppresses the postfix `if` handler so that
    /// `if` in a binder context is parsed as a guard clause, not as
    /// a conditioned expression on the binder's domain.
    suppress_postfix_if: bool,
    /// U1: when true, suppresses `|` as the `or` operator so that
    /// `|` in a `cases` body is parsed as an arm delimiter, not as
    /// a binary `or` on the arm value.
    suppress_pipe_or: bool,
}

impl Parser {
    fn new(source: &str, file: FileId, limits: &Limits) -> Self {
        let (tokens, lex_diagnostics) = lex(source, file, limits);
        Self {
            tokens,
            pos: 0,
            diagnostics: Diagnostics::new(),
            lex_diagnostics,
            tree_items: Vec::new(),
            notations: BTreeMap::new(),
            suppress_postfix_if: false,
            suppress_pipe_or: false,
        }
    }

    // ---- notation pre-scan ---------------------------------------------

    /// Collect every well-formed `notation` item into `self.notations`
    /// before the main item pass parses any expression body, so a glyph
    /// works regardless of where its declaration sits in the file (N1
    /// package scope). Malformed declarations are skipped here without
    /// diagnostics; `parse_notation_item` reports their syntax errors
    /// exactly once during the main pass.
    fn pre_scan_notations(&mut self) {
        let mut i = 0;
        // `notation` is a top-level item: only treat an ident as the start
        // of a declaration when it begins a line at indentation depth 0
        // (previous significant token is layout or nothing). An ident
        // named `notation` inside an expression or an indented suite must
        // not trigger the scan.
        let mut at_item_start = true;
        let mut depth = 0_usize;
        while i < self.tokens.len() {
            match &self.tokens[i].kind {
                TokenKind::Newline => {
                    at_item_start = true;
                    i += 1;
                    continue;
                }
                TokenKind::Indent => {
                    depth += 1;
                    at_item_start = true;
                    i += 1;
                    continue;
                }
                TokenKind::Dedent => {
                    depth = depth.saturating_sub(1);
                    at_item_start = true;
                    i += 1;
                    continue;
                }
                TokenKind::Ident(name) if name == "notation" && at_item_start && depth == 0 => {
                    if let Some((op, spellings, source)) = self.scan_notation_at(i) {
                        self.register_notation(op, spellings, source);
                        // Fast-forward past the declaration tokens; the
                        // main pass re-parses them into the tree.
                        while i + 1 < self.tokens.len()
                            && !matches!(
                                self.tokens[i + 1].kind,
                                TokenKind::Newline | TokenKind::Dedent | TokenKind::Eof
                            )
                        {
                            i += 1;
                        }
                    }
                    i += 1;
                    continue;
                }
                _ => {}
            }
            at_item_start = false;
            i += 1;
        }
    }

    /// Token-level parse of one `notation` declaration starting at
    /// `tokens[index] == Ident("notation")`. Returns the operator, the
    /// glyph spellings (canonical + optional alias) and the span, or
    /// `None` when the token shape is malformed (main pass diagnoses it).
    fn scan_notation_at(&self, index: usize) -> Option<(NotOp, Vec<String>, Span)> {
        let start = self.tokens[index].span;
        let mut j = index + 1;
        let fixity = match self.tokens.get(j).map(|token| &token.kind) {
            Some(TokenKind::Ident(name)) => match name.as_str() {
                "prefix" => NotationFixity::Prefix,
                "postfix" => NotationFixity::Postfix,
                "infixl" => NotationFixity::InfixLeft,
                "infixr" => NotationFixity::InfixRight,
                "infix" => NotationFixity::Infix,
                _ => return None,
            },
            _ => return None,
        };
        j += 1;
        let precedence = match self.tokens.get(j).map(|token| &token.kind) {
            Some(TokenKind::Int(text)) => text.parse::<u32>().ok()?,
            _ => return None,
        };
        j += 1;
        let glyph = match self.tokens.get(j).map(|token| &token.kind) {
            Some(TokenKind::Str(text)) => text.clone(),
            _ => return None,
        };
        j += 1;
        if !matches!(self.tokens.get(j).map(|token| &token.kind), Some(TokenKind::Arrow)) {
            return None;
        }
        j += 1;
        let mut target = Vec::new();
        loop {
            match self.tokens.get(j).map(|token| &token.kind) {
                Some(TokenKind::Ident(name)) => {
                    // `alias` followed by a string starts the N2 clause.
                    if name == "alias"
                        && matches!(self.tokens.get(j + 1).map(|t| &t.kind), Some(TokenKind::Str(_)))
                    {
                        break;
                    }
                    target.push(name.clone());
                    j += 1;
                }
                // Keyword segments are allowed in canonical operator paths
                // so the documented `core::logic::not` target parses (`not`
                // is a keyword token; the desugared call still resolves).
                Some(TokenKind::Keyword(keyword)) => {
                    target.push(keyword.spelling().to_string());
                    j += 1;
                }
                Some(TokenKind::PathSep | TokenKind::Dot) => {
                    j += 1;
                }
                _ => break,
            }
        }
        if target.is_empty() {
            return None;
        }
        let alias = if matches!(
            self.tokens.get(j).map(|token| &token.kind),
            Some(TokenKind::Ident(name)) if name == "alias"
        ) {
            match self.tokens.get(j + 1).map(|token| &token.kind) {
                Some(TokenKind::Str(text)) => {
                    let text = text.clone();
                    j += 2; // consume `alias` and its string
                    Some(text)
                }
                _ => return None,
            }
        } else {
            None
        };
        let end = self.tokens[j.saturating_sub(1)].span;
        let mut spellings = Vec::with_capacity(2);
        spellings.push(glyph);
        if let Some(alias) = alias {
            spellings.push(alias);
        }
        Some((
            NotOp {
                fixity,
                precedence,
                target,
            },
            spellings,
            start.cover(end),
        ))
    }

    /// Mount one scanned operator under its glyph spellings, enforcing N3
    /// (reserved glyphs), N4 (same glyph, different target in one scope),
    /// the custom-operator precedence floor, and the Phase 1 glyph lexical
    /// rule (a glyph must lex as a single identifier). First declaration
    /// wins on benign duplicates so a scope maps each glyph to exactly one
    /// operator.
    fn register_notation(&mut self, op: NotOp, spellings: Vec<String>, source: Span) {
        if op.precedence < CUSTOM_OP_MIN_PRECEDENCE {
            // The custom-operator infix layer only binds declarations at
            // or above the floor (the core ladder owns 1..=10); a lower
            // declaration would parse without ever binding, so it is
            // refused up front instead of silently doing nothing.
            self.diagnostics.error(
                "E-NOTATION-PRECEDENCE",
                format!(
                    "notation precedence {} is below the custom-operator floor {}: \
                     precedences 1..=10 belong to the core lexical ladder and cannot \
                     be renumbered by notation declarations",
                    op.precedence, CUSTOM_OP_MIN_PRECEDENCE
                ),
                source,
            );
            return;
        }
        for spelling in spellings {
            if NOTATION_RESERVED_GLYPHS.contains(&spelling.as_str()) {
                self.diagnostics.error(
                    "E-NOTATION-RESERVED",
                    format!("glyph `{spelling}` is reserved by the core language and cannot be rebound as notation"),
                    source,
                );
                continue;
            }
            if !glyph_lexes_as_ident(&spelling) {
                self.diagnostics.error(
                    "E-NOTATION-GLYPH",
                    format!(
                        "glyph `{spelling}` must lex as a single identifier to be usable as a custom operator (letters, digits, `_`, or non-ASCII characters; not a keyword)"
                    ),
                    source,
                );
                continue;
            }
            let existing_target = self
                .notations
                .get(&spelling)
                .map(|existing| existing.target.clone());
            match existing_target {
                None => {
                    self.notations.insert(spelling, op.clone());
                }
                Some(target) if target != op.target => {
                    self.diagnostics.error(
                        "E-NOTATION-AMBIG",
                        format!(
                            "glyph `{spelling}` maps to both `{}` and `{}` in this scope; N4 requires exactly one target per glyph",
                            target.join("::"),
                            op.target.join("::")
                        ),
                        source,
                    );
                }
                Some(_) => {
                    // Benign duplicate (same glyph, same target): first
                    // declaration wins; the scope stays unambiguous.
                }
            }
        }
    }

    /// Desugar a glyph use to a plain call of the canonical target.
    /// N5: the semantic IR is notation-agnostic; an operator call admits
    /// the same regardless of which glyph invoked it.
    fn notation_call(&self, target: &[String], mut args: Vec<Expr>, source: Span) -> Expr {
        let target = target.to_vec();
        Expr {
            kind: ExprKind::Call {
                function: Box::new(Expr {
                    kind: ExprKind::Path {
                        segments: target,
                        generics: None,
                    },
                    source,
                }),
                args: std::mem::take(&mut args),
            },
            source,
        }
    }

    // ---- token helpers -------------------------------------------------

    fn peek(&self) -> &TokenKind {
        &self.tokens[self.pos.min(self.tokens.len().saturating_sub(1))].kind
    }

    fn peek_at(&self, offset: usize) -> &TokenKind {
        let index = (self.pos + offset).min(self.tokens.len().saturating_sub(1));
        &self.tokens[index].kind
    }

    fn current_span(&self) -> Span {
        self.tokens[self.pos.min(self.tokens.len().saturating_sub(1))].span
    }

    fn last_span(&self) -> Span {
        self.tokens[self
            .pos
            .saturating_sub(1)
            .min(self.tokens.len().saturating_sub(1))]
        .span
    }

    fn advance(&mut self) -> Token {
        let token = self.tokens[self.pos.min(self.tokens.len().saturating_sub(1))].clone();
        if self.pos < self.tokens.len().saturating_sub(1) {
            self.pos += 1;
        }
        token
    }

    fn eat(&mut self, kind: &TokenKind) -> bool {
        if self.peek() == kind {
            self.advance();
            true
        } else {
            false
        }
    }

    fn eat_keyword(&mut self, keyword: Keyword) -> bool {
        if matches!(self.peek(), TokenKind::Keyword(k) if k == &keyword) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn peek_reduction_call(&self) -> bool {
        matches!(
            self.peek(),
            TokenKind::Keyword(Keyword::Sum | Keyword::Product)
        ) && matches!(self.peek_at(1), TokenKind::LParen)
    }

    fn error_here(&mut self, code: &'static str, message: impl Into<String>) {
        let span = self.current_span();
        self.diagnostics.error(code, message, span);
    }

    fn at_line_end(&self) -> bool {
        matches!(
            self.peek(),
            TokenKind::Newline | TokenKind::Dedent | TokenKind::Eof
        )
    }

    /// Consume newline tokens after a statement.
    fn finish_line(&mut self) {
        while matches!(self.peek(), TokenKind::Newline) {
            self.advance();
        }
    }

    /// After `=` a newline + indented expression is a continuation; consume
    /// the layout and report whether an `Indent` was taken so the caller can
    /// balance the matching `Dedent`.
    fn skip_assignment_layout(&mut self) -> bool {
        if self.peek() == &TokenKind::Newline {
            self.skip_newlines();
            if matches!(self.peek(), TokenKind::Indent) {
                self.advance();
                return true;
            }
        }
        false
    }

    fn close_assignment_indent(&mut self, opened: bool) {
        if !opened {
            return;
        }
        if self.peek() == &TokenKind::Newline {
            self.skip_newlines();
        }
        if matches!(self.peek(), TokenKind::Dedent) {
            self.advance();
        }
    }

    fn skip_newlines(&mut self) {
        while self.peek() == &TokenKind::Newline {
            self.advance();
        }
    }

    /// Inside an operator loop: if the next line begins with a continuation
    /// operator, consume its layout and return true (caller re-checks the
    /// operator). Otherwise restore and return false (expression ended).
    fn skip_continuation_lines(&mut self) -> bool {
        if self.peek() != &TokenKind::Newline {
            return false;
        }
        let save = self.pos;
        self.skip_newlines();
        // Same-level operator continuation (`y = a` newline `* b`), used
        // by multi-line definitions where the operator lines sit at
        // the same indentation as the expression start.
        if is_continuation_operator(self.peek()) {
            return true;
        }
        if matches!(self.peek(), TokenKind::Indent) {
            self.advance();
            if is_continuation_operator(self.peek()) {
                return true;
            }
            self.pos = save;
            return false;
        }
        self.pos = save;
        false
    }

    fn skip_to_line_end(&mut self) {
        while !matches!(
            self.peek(),
            TokenKind::Newline | TokenKind::Dedent | TokenKind::Eof
        ) {
            self.advance();
        }
    }

    fn skip_dedents(&mut self) {
        while matches!(self.peek(), TokenKind::Dedent) {
            self.advance();
        }
    }
}

/// A glyph is usable as a custom operator only when the source spelling
/// lexes as exactly one identifier token (run of letters, digits, `_`, or
/// non-ASCII characters, not a keyword). Punctuation glyphs such as `!`
/// or `**` tokenize differently and are refused at the declaration
/// (E-NOTATION-GLYPH); they would silently parse as other syntax.
fn glyph_lexes_as_ident(glyph: &str) -> bool {
    let (tokens, diagnostics) = lex(glyph, FileId(u32::MAX), &Limits::default());
    !diagnostics.has_errors()
        && tokens.len() == 2
        && matches!(&tokens[0].kind, TokenKind::Ident(name) if name == glyph)
        && matches!(tokens[1].kind, TokenKind::Eof)
}

fn binder_kind(kind: &TokenKind) -> BinderKind {
    match kind {
        TokenKind::Keyword(Keyword::Sum) => BinderKind::Sum,
        TokenKind::Keyword(Keyword::Product) => BinderKind::Product,
        TokenKind::Keyword(Keyword::Integral) => BinderKind::Integral,
        TokenKind::Keyword(Keyword::ForAll) => BinderKind::ForAll,
        _ => BinderKind::Exists,
    }
}

fn is_continuation_operator(kind: &TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Plus
            | TokenKind::Minus
            | TokenKind::Star
            | TokenKind::Slash
            | TokenKind::Caret
            | TokenKind::EqEq
            | TokenKind::NotEq
            | TokenKind::Le
            | TokenKind::Ge
            | TokenKind::Lt
            | TokenKind::Gt
            | TokenKind::Amp
            | TokenKind::Pipe
            | TokenKind::Keyword(Keyword::And | Keyword::Or)
    )
}

fn comparison_operator(kind: &TokenKind) -> Option<BinaryOp> {
    match kind {
        TokenKind::EqEq => Some(BinaryOp::Eq),
        TokenKind::NotEq => Some(BinaryOp::Ne),
        TokenKind::Lt => Some(BinaryOp::Lt),
        TokenKind::Le => Some(BinaryOp::Le),
        TokenKind::Gt => Some(BinaryOp::Gt),
        TokenKind::Ge => Some(BinaryOp::Ge),
        _ => None,
    }
}

fn visibility_name(visibility: Visibility) -> &'static str {
    match visibility {
        Visibility::Public => "public",
        Visibility::Package => "package",
        Visibility::Private => "private",
    }
}

// Declarations keep their full ordered body (`Declaration.body`); sections
// are a filtered view via `Declaration::sections()`.

fn suite_has_section(suite: &Suite, name: &str) -> bool {
    suite
        .statements
        .iter()
        .any(|stmt| matches!(&stmt.kind, StmtKind::Section(section) if section.name == name))
}
