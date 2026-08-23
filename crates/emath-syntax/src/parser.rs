//! Bootstrap recursive-descent parser for the `.emath` surface.
//!
//! Covers the full structural grammar of the corpus (sections, generics,
//! records, commands, precedence expressions, binders, quantity literals,
//! chained comparisons, continuation lines). Phase 1 semantics admits a
//! subset; the parser itself accepts the documented surface. Never panics;
//! recovers at statement boundaries; spans everywhere.

use crate::lexer::lex;
use crate::token::{Keyword, Token, TokenKind};
use crate::tree::{
    BinaryOp, BinderKind, Item, StmtKind, Suite, SyntaxTree, Visibility,
};
use emath_core::{limits::Limits, Diagnostics, FileId, Span};

mod decl;
mod expr;
mod stmt;
mod stmt_binders;
mod stmt_idents;
mod stmt_suite;
mod types;

const MAX_EXPR_DEPTH: usize = 128;

/// Parse an in-memory source into a syntax tree.
#[must_use]
pub fn parse(source: &str, file: FileId, limits: &Limits) -> (SyntaxTree, Diagnostics) {
    let mut parser = Parser::new(source, file, limits);
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

    /// After `=` (assignment/default/let) a newline + indented expression is
    /// a continuation. Consume the layout unconditionally.
    ///
    /// Returns true when an `Indent` was consumed so the caller can
    /// balance the matching `Dedent`. Otherwise the next sibling section
    /// (at the enclosing indent) emits two Dedents in a row and the
    /// parent suite closes early.
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
