//! Bootstrap recursive-descent parser for the `.emath` surface.
//!
//! Covers the full structural grammar of the V5 corpus (sections, generics,
//! records, commands, precedence expressions, binders, quantity literals,
//! chained comparisons, continuation lines). Phase 1 semantics admits a
//! subset; the parser itself accepts the documented surface. Never panics;
//! recovers at statement boundaries; spans everywhere.

use crate::lexer::lex;
use crate::token::{Keyword, Token, TokenKind};
use crate::tree::{
    Argument, ArgumentValue, BinaryOp, Binder, BinderKind, CommandArgument, Declaration,
    DeclarationSignature, Expr, ExprKind, GenericParam, Item, Param, Place, Section, Stmt,
    StmtKind, Suite, SyntaxTree, TypeExpr, TypeKind, UnaryOp, UseTree, Visibility,
};
use emath_core::{limits::Limits, Diagnostics, FileId, Span};

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
    fn skip_assignment_layout(&mut self) {
        if self.peek() == &TokenKind::Newline {
            self.skip_newlines();
            if matches!(self.peek(), TokenKind::Indent) {
                self.advance();
            }
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
        // by V6 multi-line definitions where the operator lines sit at
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

    // ---- items ---------------------------------------------------------

    fn parse_items(&mut self) {
        self.finish_line();
        while self.peek() != &TokenKind::Eof {
            match self.peek() {
                TokenKind::Keyword(Keyword::Package) => {
                    if let Some(item) = self.parse_package_item() {
                        self.tree_items.push(item);
                    }
                }
                TokenKind::Keyword(Keyword::Use) => {
                    if let Some(item) = self.parse_use_item() {
                        self.tree_items.push(item);
                    }
                }
                TokenKind::Keyword(Keyword::Emath) => match self.parse_declaration() {
                    Some(decl) => self.tree_items.push(Item::Declaration(decl)),
                    None => self.skip_to_line_end(),
                },
                TokenKind::Keyword(Keyword::Extern) => match self.parse_extern_item() {
                    Some(decl) => self.tree_items.push(Item::Declaration(decl)),
                    None => self.skip_to_line_end(),
                },
                TokenKind::Newline | TokenKind::Indent | TokenKind::Dedent => {
                    self.advance();
                }
                _ => {
                    self.error_here("E-SYN-101", "expected an `emath` declaration or `use` item");
                    self.skip_to_line_end();
                }
            }
            self.finish_line();
            self.skip_dedents();
        }
    }

    /// `package examples.square` — V6 package identity line.
    fn parse_package_item(&mut self) -> Option<Item> {
        let start = self.current_span();
        self.advance(); // `package`
        let mut segments = Vec::new();
        loop {
            match self.peek() {
                TokenKind::Ident(name) => {
                    segments.push(name.clone());
                    self.advance();
                }
                TokenKind::PathSep | TokenKind::Dot => {
                    self.advance();
                }
                _ => break,
            }
        }
        if segments.is_empty() {
            self.error_here("E-SYN-110", "expected a package path after `package`");
            return None;
        }
        let mut span = start.cover(self.last_span());
        if self.peek() == &TokenKind::Colon {
            span = span.cover(self.current_span());
        }
        Some(Item::Package {
            path: segments,
            source: span,
        })
    }

    fn parse_use_item(&mut self) -> Option<Item> {
        let start = self.current_span();
        self.advance(); // `use`
        let mut segments = Vec::new();
        let mut tree = None;
        loop {
            match self.peek() {
                TokenKind::Ident(name) => {
                    // `use std.units.{A, B}` — stop before a brace group.
                    if matches!(self.peek_at(1), TokenKind::LBrace) {
                        break;
                    }
                    segments.push(name.clone());
                    self.advance();
                }
                TokenKind::Keyword(Keyword::As) => {
                    self.advance();
                    if let TokenKind::Ident(_) = self.peek() {
                        self.advance();
                    }
                }
                TokenKind::Star => {
                    self.advance();
                    tree = Some(UseTree::All);
                }
                // V6 uses dotted paths (`use std.numeric.Real`).
                TokenKind::PathSep | TokenKind::Dot => {
                    self.advance();
                }
                TokenKind::LBrace => {
                    self.advance();
                    let mut names = Vec::new();
                    while !matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
                        match self.peek() {
                            TokenKind::Ident(name) => {
                                let name = name.clone();
                                self.advance();
                                let alias = if self.eat_keyword(Keyword::As) {
                                    match self.peek().clone() {
                                        TokenKind::Ident(alias) => {
                                            self.advance();
                                            Some(alias)
                                        }
                                        _ => None,
                                    }
                                } else {
                                    None
                                };
                                names.push((name, alias));
                            }
                            TokenKind::Comma => {
                                self.advance();
                            }
                            _ => {
                                self.error_here(
                                    "E-SYN-101",
                                    "expected an identifier or `,` in `use` group",
                                );
                                self.skip_to_line_end();
                                break;
                            }
                        }
                    }
                    self.eat(&TokenKind::RBrace);
                    tree = Some(UseTree::Named(names));
                }
                _ => break,
            }
        }
        if segments.is_empty() && tree.is_none() {
            self.error_here("E-SYN-110", "expected a path after `use`");
            return None;
        }
        Some(Item::Use {
            path: segments,
            tree: tree.unwrap_or(UseTree::Named(Vec::new())),
            source: start.cover(self.last_span()),
        })
    }

    /// Declaration heads (both dialects):
    /// - V5 legacy: `emath custom <Name<Params>> as kind:` `suite`
    /// - V6: `emath function Square<T: Real>:`, `emath record CacheCandidate:`,
    ///   `emath RankingPolicy FreshnessScore:` (custom-kind use)
    fn parse_declaration(&mut self) -> Option<Declaration> {
        let start = self.current_span();
        self.advance(); // `emath`
                        // The declaration kind is the next word (`custom`,
                        // `function`, `policy`, `record`, `model`, `kind`,
                        // `search`, `experiment`, `type`, or a user kind).
        let item_kind = match self.peek().clone() {
            TokenKind::Ident(item_kind) => item_kind,
            TokenKind::Keyword(Keyword::Custom) => "custom".to_string(),
            _ => {
                self.error_here("E-SYN-101", "expected a declaration kind after `emath`");
                return None;
            }
        };
        self.advance();
        // Legacy `emath custom <Name>`: the name is angle-bracketed and the
        // outer `>` is closed after any inner generics.
        let legacy_bracketed = self.eat(&TokenKind::Lt);
        let name = if legacy_bracketed {
            let TokenKind::Ident(name) = self.peek().clone() else {
                self.error_here("E-SYN-101", "expected a declaration name after `<`");
                return None;
            };
            self.advance();
            name
        } else {
            let TokenKind::Ident(name) = self.peek().clone() else {
                self.error_here("E-SYN-101", "expected a declaration name");
                return None;
            };
            self.advance();
            name
        };
        let generics = if matches!(self.peek(), TokenKind::Lt) {
            self.parse_generic_params()
        } else {
            Vec::new()
        };
        if legacy_bracketed && !self.eat(&TokenKind::Gt) {
            self.error_here("E-SYN-102", "expected `>` to close the declaration name");
            return None;
        }
        let mut as_kind = String::new();
        if self.eat_keyword(Keyword::As) {
            match self.peek().clone() {
                TokenKind::Ident(kind) => {
                    as_kind = kind;
                    self.advance();
                }
                _ => {
                    self.error_here("E-SYN-110", "expected a kind name after `as`");
                }
            }
        }
        if !self.eat(&TokenKind::Colon) {
            self.error_here("E-SYN-111", "expected `:` after the declaration head");
            return None;
        }
        let suite = self.parse_suite()?;
        Some(Declaration {
            name,
            generics,
            item_kind,
            as_kind,
            attributes: Vec::new(),
            body: suite.statements,
            signature: None,
            source: start.cover(self.last_span()),
            head_source: start.cover(self.last_span()),
        })
    }

    /// Top-level `extern operator name<Generics>(params) -> Ret:` `suite`
    /// (V6 `09_parametric_provider`). Becomes a declaration of kind
    /// `extern` / `operator` so the rest of the pipeline sees one shape.
    fn parse_extern_item(&mut self) -> Option<Declaration> {
        let start = self.current_span();
        self.advance(); // `extern`
        let as_kind = match self.peek().clone() {
            TokenKind::Ident(what) => {
                self.advance();
                what
            }
            _ => {
                self.error_here("E-SYN-110", "expected `operator` or `fn` after `extern`");
                return None;
            }
        };
        let TokenKind::Ident(name) = self.peek().clone() else {
            self.error_here("E-SYN-110", "expected an operator name after `extern`");
            return None;
        };
        self.advance();
        let generics = if matches!(self.peek(), TokenKind::Lt) {
            self.parse_generic_params()
        } else {
            Vec::new()
        };
        let (params, ret) = self.parse_params_after_name()?;
        let suite = if self.eat(&TokenKind::Colon) {
            self.parse_suite()
        } else {
            None
        };
        let source = start.cover(self.last_span());
        Some(Declaration {
            name,
            generics,
            item_kind: "extern".to_string(),
            as_kind,
            attributes: Vec::new(),
            body: suite.map_or_else(Vec::new, |suite| suite.statements),
            signature: Some(DeclarationSignature { params, ret }),
            head_source: source,
            source,
        })
    }

    fn parse_generic_params(&mut self) -> Vec<GenericParam> {
        let mut params = Vec::new();
        if !self.eat(&TokenKind::Lt) {
            return params;
        }
        loop {
            if matches!(self.peek(), TokenKind::Gt) {
                self.advance();
                break;
            }
            let start = self.current_span();
            let TokenKind::Ident(name) = self.peek().clone() else {
                self.error_here("E-SYN-101", "expected a generic parameter name");
                break;
            };
            self.advance();
            let bound = if self.eat(&TokenKind::Colon) {
                self.parse_type_expr()
            } else {
                None
            };
            params.push(GenericParam {
                name,
                bound,
                source: start.cover(self.last_span()),
            });
            if self.eat(&TokenKind::Comma) {
                continue;
            }
            if matches!(self.peek(), TokenKind::Gt) {
                self.advance();
                break;
            }
            self.error_here("E-SYN-101", "expected `,` or `>` in generic parameter list");
            break;
        }
        params
    }

    // ---- suites --------------------------------------------------------

    fn parse_suite(&mut self) -> Option<Suite> {
        let start = self.current_span();
        if self.at_line_end() {
            self.finish_line();
            if !matches!(self.peek(), TokenKind::Indent) {
                self.error_here("E-SYN-112", "expected an indented block");
                return None;
            }
            self.advance();
            let mut statements = Vec::new();
            while !matches!(self.peek(), TokenKind::Dedent | TokenKind::Eof) {
                if matches!(self.peek(), TokenKind::Newline) {
                    self.advance();
                    continue;
                }
                match self.parse_statement() {
                    Some(stmt) => statements.push(stmt),
                    None => self.skip_to_line_end(),
                }
                self.finish_line();
            }
            let end = self.current_span();
            self.advance(); // Dedent
            Some(Suite {
                statements,
                source: start.cover(end),
            })
        } else {
            let mut statements = Vec::new();
            while !self.at_line_end() {
                if let Some(stmt) = self.parse_statement() {
                    statements.push(stmt);
                } else {
                    self.skip_to_line_end();
                    break;
                }
            }
            self.finish_line();
            Some(Suite {
                statements,
                source: start.cover(self.last_span()),
            })
        }
    }

    fn stmt(&self, start: Span, kind: StmtKind) -> Stmt {
        Stmt {
            kind,
            source: start.cover(self.last_span()),
        }
    }

    fn parse_statement(&mut self) -> Option<Stmt> {
        let start = self.current_span();
        match self.peek().clone() {
            TokenKind::Keyword(Keyword::Require) => {
                self.advance();
                self.skip_assignment_layout();
                let expr = self.parse_loose_expr()?;
                Some(self.stmt(start, StmtKind::Require(expr)))
            }
            TokenKind::Keyword(Keyword::Ensure) => {
                self.advance();
                self.skip_assignment_layout();
                let expr = self.parse_loose_expr()?;
                Some(self.stmt(start, StmtKind::Ensure(expr)))
            }
            TokenKind::Keyword(Keyword::Invariant) => {
                if self.peek_at(1) == &TokenKind::Colon {
                    // `invariant:` V6 section head
                    self.advance();
                    self.advance();
                    let suite = self.parse_suite()?;
                    Some(self.stmt(
                        start,
                        StmtKind::Section(Section {
                            name: "invariant".into(),
                            generic: None,
                            args: None,
                            suite,
                            source: start.cover(self.last_span()),
                            head_source: start.cover(self.last_span()),
                        }),
                    ))
                } else {
                    self.advance();
                    self.skip_assignment_layout();
                    let expr = self.parse_expr()?;
                    Some(self.stmt(start, StmtKind::Invariant(expr)))
                }
            }
            TokenKind::Keyword(Keyword::And) => {
                self.advance();
                let expr = self.parse_loose_expr()?;
                Some(self.stmt(
                    start,
                    StmtKind::Command {
                        head: vec!["and".to_string()],
                        argument: Some(CommandArgument::Expr(expr)),
                    },
                ))
            }
            TokenKind::Keyword(Keyword::Or) => {
                self.advance();
                let expr = self.parse_loose_expr()?;
                Some(self.stmt(
                    start,
                    StmtKind::Command {
                        head: vec!["or".to_string()],
                        argument: Some(CommandArgument::Expr(expr)),
                    },
                ))
            }
            TokenKind::Keyword(
                Keyword::Where
                | Keyword::Wrt
                | Keyword::Over
                | Keyword::Against
                | Keyword::With
                | Keyword::At
                | Keyword::On,
            ) => {
                let head_word = match self.peek() {
                    TokenKind::Keyword(k) => k.spelling().to_string(),
                    _ => String::new(),
                };
                self.advance();
                let (head, argument) = self.parse_command_tail(vec![head_word])?;
                Some(self.stmt(start, StmtKind::Command { head, argument }))
            }
            TokenKind::Keyword(Keyword::Let) => {
                self.advance();
                let TokenKind::Ident(name) = self.peek().clone() else {
                    self.error_here("E-SYN-110", "expected a name after `let`");
                    return None;
                };
                self.advance();
                let ty = if self.eat(&TokenKind::Colon) {
                    self.parse_type_expr()
                } else {
                    None
                };
                if !self.eat(&TokenKind::Eq) {
                    self.error_here("E-SYN-111", "expected `=` in `let` binding");
                    return None;
                }
                self.skip_assignment_layout();
                let value = self.parse_expr()?;
                Some(self.stmt(start, StmtKind::Let { name, ty, value }))
            }
            TokenKind::Keyword(Keyword::If) => self.parse_if_statement(start),
            TokenKind::Keyword(Keyword::SelfKw) => self.parse_self_block(start),
            TokenKind::Keyword(Keyword::Fn) => self.parse_fn_statement(start, None),
            TokenKind::Keyword(Keyword::Extern) => {
                self.advance();
                let TokenKind::Ident(what) = self.peek().clone() else {
                    self.error_here("E-SYN-110", "expected `fn` or `operator` after `extern`");
                    return None;
                };
                self.advance();
                if what == "operator" {
                    let (name, params, ret) = self.parse_params_header()?;
                    Some(self.stmt(
                        start,
                        StmtKind::OperatorDecl {
                            name,
                            params,
                            ret,
                            source: start.cover(self.last_span()),
                        },
                    ))
                } else if what == "fn" {
                    self.parse_fn_statement(start, None)
                } else {
                    self.error_here("E-SYN-101", "expected `fn` or `operator` after `extern`");
                    None
                }
            }
            TokenKind::Keyword(
                Keyword::Sum
                | Keyword::Product
                | Keyword::Integral
                | Keyword::ForAll
                | Keyword::Exists,
            ) => self.parse_binder_statement(start),
            TokenKind::Keyword(Keyword::Pub | Keyword::Package | Keyword::Private) => {
                let visibility = match self.peek() {
                    TokenKind::Keyword(Keyword::Pub) => Visibility::Public,
                    TokenKind::Keyword(Keyword::Package) => Visibility::Package,
                    _ => Visibility::Private,
                };
                self.advance();
                if matches!(self.peek(), TokenKind::Keyword(Keyword::Fn)) {
                    self.parse_fn_statement(start, Some(visibility))
                } else {
                    let (head, argument) =
                        self.parse_command_tail(vec![visibility_name(visibility).to_string()])?;
                    Some(self.stmt(start, StmtKind::Command { head, argument }))
                }
            }
            TokenKind::Ident(name) => self.parse_ident_statement(start, name),
            TokenKind::Int(_)
            | TokenKind::Float(_)
            | TokenKind::Str(_)
            | TokenKind::Minus
            | TokenKind::Bang
            | TokenKind::LParen
            | TokenKind::LBracket
            | TokenKind::Keyword(
                Keyword::True | Keyword::False | Keyword::Derivative,
            ) => {
                let expr = self.parse_expr()?;
                if let Some(stmt) = self.parse_equation_tail(&expr, start) {
                    return Some(stmt);
                }
                Some(self.stmt(start, StmtKind::Expr(expr)))
            }
            other => {
                self.error_here("E-SYN-101", format!("unexpected {}", other.describe()));
                None
            }
        }
    }

    /// `require section inputs`, `require guarded`: loose path acceptance.
    fn parse_loose_expr(&mut self) -> Option<Expr> {
        let start = self.current_span();
        let mut expr = self.parse_expr()?;
        // join space-separated identifiers onto a path
        let mut joined = false;
        if let ExprKind::Path {
            segments,
            generics: None,
        } = &expr.kind
        {
            let mut segments = segments.clone();
            loop {
                match self.peek() {
                    TokenKind::Ident(next) => {
                        if matches!(
                            self.peek_at(1),
                            TokenKind::Ident(_)
                                | TokenKind::Newline
                                | TokenKind::PathSep
                                | TokenKind::Dot
                        ) {
                            segments.push(next.clone());
                            self.advance();
                            joined = true;
                        } else {
                            break;
                        }
                    }
                    TokenKind::PathSep | TokenKind::Dot => {
                        if matches!(self.peek_at(1), TokenKind::Ident(_)) {
                            self.advance();
                            if let TokenKind::Ident(next) = self.peek().clone() {
                                segments.push(next);
                                self.advance();
                                joined = true;
                            } else {
                                break;
                            }
                        } else {
                            break;
                        }
                    }
                    _ => break,
                }
            }
            if joined {
                expr = Expr {
                    kind: ExprKind::Path {
                        segments,
                        generics: None,
                    },
                    source: start.cover(self.last_span()),
                };
            }
        }
        // trailing operator continuation (`require something >= 0`)
        loop {
            let op = match self.peek() {
                TokenKind::EqEq => BinaryOp::Eq,
                TokenKind::NotEq => BinaryOp::Ne,
                TokenKind::Lt => BinaryOp::Lt,
                TokenKind::Le => BinaryOp::Le,
                TokenKind::Gt => BinaryOp::Gt,
                TokenKind::Ge => BinaryOp::Ge,
                _ => break,
            };
            self.advance();
            let right = self.parse_expr()?;
            let span = expr.source.cover(right.source);
            expr = Expr {
                kind: ExprKind::Binary {
                    op,
                    left: Box::new(expr),
                    right: Box::new(right),
                },
                source: span,
            };
        }
        Some(expr)
    }

    fn parse_if_statement(&mut self, start: Span) -> Option<Stmt> {
        self.advance(); // if
        let condition = self.parse_expr()?;
        if !self.eat(&TokenKind::Colon) {
            self.error_here("E-SYN-111", "expected `:` after `if` condition");
            return None;
        }
        let then = self.parse_suite()?;
        let mut else_branches = Vec::new();
        let mut else_tail = None;
        loop {
            if !self.eat_keyword(Keyword::Else) {
                break;
            }
            if self.eat_keyword(Keyword::If) {
                let cond = self.parse_expr()?;
                if !self.eat(&TokenKind::Colon) {
                    self.error_here("E-SYN-111", "expected `:` after `else if` condition");
                    return None;
                }
                let suite = self.parse_suite()?;
                else_branches.push((cond, suite));
            } else {
                if !self.eat(&TokenKind::Colon) {
                    self.error_here("E-SYN-111", "expected `:` after `else`");
                    return None;
                }
                else_tail = self.parse_suite();
                break;
            }
        }
        Some(self.stmt(
            start,
            StmtKind::If {
                condition,
                then,
                else_branches,
                else_tail,
            },
        ))
    }

    fn parse_self_block(&mut self, start: Span) -> Option<Stmt> {
        self.advance(); // Self
        if !self.eat(&TokenKind::Colon) {
            self.error_here("E-SYN-111", "expected `:` after `Self`");
            return None;
        }
        let suite = self.parse_suite()?;
        let mut assignments = Vec::new();
        for stmt in &suite.statements {
            match &stmt.kind {
                StmtKind::Assign { target, value } if target.segments.len() == 1 => {
                    assignments.push((target.segments[0].clone(), value.clone()));
                }
                StmtKind::FieldDecl { name, default, .. } => {
                    if let Some(value) = default {
                        assignments.push((name.clone(), value.clone()));
                    }
                }
                _ => {
                    self.diagnostics.error(
                        "E-SYN-101",
                        "only `name = expr` assignments are allowed in a `Self:` block",
                        stmt.source,
                    );
                }
            }
        }
        Some(Stmt {
            kind: StmtKind::SelfBlock { assignments },
            source: start.cover(suite.source),
        })
    }

    /// `fn name(params) [-> Ret] [:] suite`
    fn parse_fn_statement(&mut self, start: Span, visibility: Option<Visibility>) -> Option<Stmt> {
        self.advance(); // `fn`
        let TokenKind::Ident(name) = self.peek().clone() else {
            self.error_here("E-SYN-110", "expected a function name after `fn`");
            return None;
        };
        self.advance();
        let (params, ret) = self.parse_params_after_name()?;
        let suite = if self.eat(&TokenKind::Colon) {
            self.parse_suite()
        } else {
            None
        };
        Some(self.stmt(
            start,
            StmtKind::FnDecl {
                visibility,
                head: "fn".to_string(),
                name,
                params,
                ret,
                suite,
                source: start.cover(self.last_span()),
            },
        ))
    }

    fn parse_params_header(&mut self) -> Option<(String, Vec<Param>, Option<TypeExpr>)> {
        let TokenKind::Ident(name) = self.peek().clone() else {
            self.error_here("E-SYN-110", "expected a name");
            return None;
        };
        self.advance();
        // `extern operator semantic_distance<D: Nat>(...)` generics.
        if matches!(self.peek(), TokenKind::Lt) {
            let _ = self.parse_generic_params();
        }
        self.parse_params_after_name()
            .map(|(params, ret)| (name, params, ret))
    }

    fn parse_params_after_name(&mut self) -> Option<(Vec<Param>, Option<TypeExpr>)> {
        self.parse_params_after_name_flag(false)
    }

    /// `allow_untyped` accepts `name` without `: Type` (method-style defines
    /// like `define score(candidate) -> Real:`), synthesizing an `Infer`
    /// marker type so the tree stays typed.
    fn parse_params_after_name_flag(
        &mut self,
        allow_untyped: bool,
    ) -> Option<(Vec<Param>, Option<TypeExpr>)> {
        let mut params = Vec::new();
        if !self.eat(&TokenKind::LParen) {
            self.error_here("E-SYN-101", "expected `(` for parameter list");
            return None;
        }
        while !matches!(self.peek(), TokenKind::RParen | TokenKind::Eof) {
            if self.eat(&TokenKind::Comma) {
                continue;
            }
            let start = self.current_span();
            let by_ref = self.eat(&TokenKind::Amp);
            let TokenKind::Ident(name) = self.peek().clone() else {
                self.error_here("E-SYN-101", "expected a parameter name");
                break;
            };
            self.advance();
            let ty = if self.eat(&TokenKind::Colon) {
                let Some(ty) = self.parse_type_expr() else {
                    break;
                };
                ty
            } else if allow_untyped {
                TypeExpr {
                    kind: TypeKind::Path {
                        segments: vec!["Infer".into()],
                        generic_args: vec![],
                    },
                    source: start.cover(self.last_span()),
                }
            } else {
                self.error_here("E-SYN-111", "expected `:` after parameter name");
                break;
            };
            let default = if self.eat(&TokenKind::Eq) {
                self.parse_expr()
            } else {
                None
            };
            params.push(Param {
                name,
                ty,
                by_ref,
                default,
                source: start.cover(self.last_span()),
            });
        }
        if !self.eat(&TokenKind::RParen) {
            self.error_here("E-SYN-102", "expected `)` to close parameter list");
            return None;
        }
        let ret = if self.eat(&TokenKind::Arrow) {
            self.parse_type_expr()
        } else {
            None
        };
        Some((params, ret))
    }

    fn parse_binder_statement(&mut self, start: Span) -> Option<Stmt> {
        let kind = binder_kind(self.peek());
        self.advance();
        let binders = self.parse_binders()?;
        if self.eat(&TokenKind::Colon) {
            let suite = self.parse_suite()?;
            Some(self.stmt(
                start,
                StmtKind::BinderStmt {
                    kind,
                    binders,
                    suite,
                },
            ))
        } else {
            self.skip_assignment_layout();
            let body = self.parse_expr()?;
            let expr = Expr {
                kind: ExprKind::Binder {
                    kind,
                    binders,
                    body: Box::new(body),
                },
                source: start.cover(self.last_span()),
            };
            Some(self.stmt(start, StmtKind::Expr(expr)))
        }
    }

    fn parse_binders(&mut self) -> Option<Vec<Binder>> {
        let mut binders = Vec::new();
        loop {
            let start = self.current_span();
            let TokenKind::Ident(name) = self.peek().clone() else {
                self.error_here("E-SYN-110", "expected a binder variable name");
                return None;
            };
            self.advance();
            let domain = if self.eat_keyword(Keyword::In) {
                self.parse_expr()
            } else {
                None
            };
            binders.push(Binder {
                name,
                domain,
                source: start.cover(self.last_span()),
            });
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        Some(binders)
    }

    // ---- ident-headed statements --------------------------------------

    fn parse_ident_statement(&mut self, start: Span, name: String) -> Option<Stmt> {
        match name.as_str() {
            "record" | "variant" | "trait" | "implementation" | "predicate" => {
                self.advance();
                let TokenKind::Ident(decl_name) = self.peek().clone() else {
                    self.error_here("E-SYN-110", "expected a name after `{name}`");
                    return None;
                };
                self.advance();
                let mut generic = Some(decl_name);
                // `predicate <candidate>(w: Witness):` form
                if matches!(self.peek(), TokenKind::Lt) && name != "implementation" {
                    self.advance();
                    if let TokenKind::Ident(inner) = self.peek().clone() {
                        generic = Some(inner);
                        self.advance();
                    }
                    self.eat(&TokenKind::Gt);
                }
                let args = if matches!(self.peek(), TokenKind::LParen) {
                    self.parse_arguments()
                } else {
                    None
                };
                if !self.eat(&TokenKind::Colon) {
                    self.error_here("E-SYN-111", "expected `:` after declaration name");
                    return None;
                }
                let suite = self.parse_suite()?;
                Some(self.stmt(
                    start,
                    StmtKind::Section(Section {
                        name,
                        generic,
                        args,
                        suite,
                        source: start.cover(self.last_span()),
                        head_source: start.cover(self.last_span()),
                    }),
                ))
            }
            "type" => {
                self.advance();
                let TokenKind::Ident(alias) = self.peek().clone() else {
                    self.error_here("E-SYN-110", "expected a type name after `type`");
                    return None;
                };
                self.advance();
                if self.eat(&TokenKind::Eq) {
                    let _ = self.parse_type_expr()?;
                    Some(self.stmt(
                        start,
                        StmtKind::Command {
                            head: vec!["type".to_string(), alias],
                            argument: None,
                        },
                    ))
                } else if self.eat(&TokenKind::Colon) {
                    let suite = self.parse_suite()?;
                    Some(self.stmt(
                        start,
                        StmtKind::Section(Section {
                            name: "type".to_string(),
                            generic: Some(alias),
                            args: None,
                            suite,
                            source: start.cover(self.last_span()),
                            head_source: start.cover(self.last_span()),
                        }),
                    ))
                } else {
                    self.error_here("E-SYN-101", "expected `=` or `:` after type name");
                    None
                }
            }
            "given" => {
                self.advance();
                let TokenKind::Ident(given_name) = self.peek().clone() else {
                    self.error_here("E-SYN-110", "expected a name after `given`");
                    return None;
                };
                self.advance();
                if !self.eat(&TokenKind::Eq) {
                    self.error_here("E-SYN-111", "expected `=` in `given` binding");
                    return None;
                }
                self.skip_assignment_layout();
                let value = self.parse_expr()?;
                Some(self.stmt(
                    start,
                    StmtKind::Given {
                        name: given_name,
                        value,
                    },
                ))
            }
            "expect" => {
                self.advance();
                self.skip_assignment_layout();
                let expr = self.parse_expr()?;
                Some(self.stmt(start, StmtKind::Expect(expr)))
            }
            "extend" => {
                // `extends model` style: verify corpus usage once
                let segments = self.collect_spaced_idents();
                Some(self.stmt(
                    start,
                    StmtKind::Command {
                        head: segments,
                        argument: None,
                    },
                ))
            }
            _ => self.parse_default_ident_statement(start),
        }
    }

    /// Generic ident-headed statement: sections, fields, commands, assigns.
    fn parse_default_ident_statement(&mut self, start: Span) -> Option<Stmt> {
        let TokenKind::Ident(name) = self.peek().clone() else {
            return None;
        };

        // `evaluate <score>:` section heads — but not comparisons
        // (`a < b < c`): the matching `>` must be followed by `:` or `(`.
        if matches!(self.peek_at(1), TokenKind::Lt)
            && self.lookahead_matches_lt_angle_head()
        {
            self.advance(); // name
            self.advance(); // <
            let TokenKind::Ident(generic) = self.peek().clone() else {
                self.error_here("E-SYN-101", "expected a name inside `< >`");
                return None;
            };
            self.advance();
            if !self.eat(&TokenKind::Gt) {
                self.error_here("E-SYN-102", "expected `>` to close section head");
                return None;
            }
            let args = if matches!(self.peek(), TokenKind::LParen) {
                self.parse_arguments()
            } else {
                None
            };
            if !self.eat(&TokenKind::Colon) {
                self.error_here("E-SYN-111", "expected `:` after section head");
                return None;
            }
            let suite = self.parse_suite()?;
            return Some(self.stmt(
                start,
                StmtKind::Section(Section {
                    name,
                    generic: Some(generic),
                    args,
                    suite,
                    source: start.cover(self.last_span()),
                    head_source: start.cover(self.last_span()),
                }),
            ));
        }

        // Two-word section heads: `goal rust:`, `tune score:`,
        // `lower declaration:`, `dispatch authority:` (V6).
        if matches!(self.peek_at(1), TokenKind::Ident(_))
            && matches!(self.peek_at(2), TokenKind::Colon)
        {
            self.advance(); // first word
            let TokenKind::Ident(generic) = self.peek().clone() else {
                return None;
            };
            self.advance();
            if !self.eat(&TokenKind::Colon) {
                self.error_here("E-SYN-111", "expected `:` after section head");
                return None;
            }
            let suite = self.parse_suite()?;
            return Some(self.stmt(
                start,
                StmtKind::Section(Section {
                    name,
                    generic: Some(generic),
                    args: None,
                    suite,
                    source: start.cover(self.last_span()),
                    head_source: start.cover(self.last_span()),
                }),
            ));
        }

        // `method score(candidate: ...) -> f64:` two-word fn declarations
        if matches!(self.peek_at(1), TokenKind::Ident(_))
            && matches!(self.peek_at(2), TokenKind::LParen)
        {

            // disambiguate from `produce rust.library` style by scanning for a
            // matching `)` followed by `->` or `:`
            if self.looks_like_params_header() {
                self.advance(); // first word
                let TokenKind::Ident(second) = self.peek().clone() else {
                    return None;
                };
                self.advance();
                let (params, ret) = self.parse_params_after_name_flag(true)?;
                let suite = if self.eat(&TokenKind::Colon) {
                    self.parse_suite()
                } else {
                    None
                };
                return Some(self.stmt(
                    start,
                    StmtKind::FnDecl {
                        visibility: None,
                        head: name,
                        name: second,
                        params,
                        ret,
                        suite,
                        source: start.cover(self.last_span()),
                    },
                ));
            }
        }

        // `candidate(candidate: &CacheCandidate) -> f64:` call or fn? — a
        // single ident + `(` that scans as a params header is a fn decl;
        // otherwise it is an expression statement.
        if matches!(self.peek_at(1), TokenKind::LParen) && self.looks_like_params_header() {
            self.advance();
            let (params, ret) = self.parse_params_after_name()?;
            let suite = if self.eat(&TokenKind::Colon) {
                self.parse_suite()
            } else {
                None
            };
            return Some(self.stmt(
                start,
                StmtKind::FnDecl {
                    visibility: None,
                    head: "fn".to_string(),
                    name,
                    params,
                    ret,
                    suite,
                    source: start.cover(self.last_span()),
                },
            ));
        }

        // Full-expression statements and equations (V6 `equation:` /
        // `constraint:` sections): `mass * derivative(velocity) = rhs`,
        // `a * a + b * b = c * c`, `a < b < c`. Trigger: the token after
        // the leading ident is an operator or a call paren, or a dotted /
        // `::` path continuation that still has an operator or `=` ahead
        // (`core.policy:` is a section head, not an expression).
        // Bare `name = value` stays an assignment.
        let op_led = matches!(
            self.peek_at(1),
            TokenKind::Star
                | TokenKind::Slash
                | TokenKind::Plus
                | TokenKind::Minus
                | TokenKind::Caret
                | TokenKind::Le
                | TokenKind::Ge
                | TokenKind::Lt
                | TokenKind::Gt
                | TokenKind::EqEq
                | TokenKind::NotEq
                | TokenKind::LParen
        );
        let dot_led = matches!(self.peek_at(1), TokenKind::Dot | TokenKind::PathSep)
            && self.dotted_continues_expression();
        if op_led || dot_led {
            let left = self.parse_expr()?;
            if let Some(stmt) = self.parse_equation_tail(&left, start) {
                return Some(stmt);
            }
            return Some(self.stmt(start, StmtKind::Expr(left)));
        }

        // `implement cache_core::Policy for Self:` host binding
        if name == "implement" {
            self.advance();
            let segments = self.collect_segments_with_dots().0;
            if segments.is_empty() {
                self.error_here("E-SYN-110", "expected a path after `implement`");
                return None;
            }
            let mut target = String::new();
            if self.eat_keyword(Keyword::For) {
                target = self.collect_segments_with_dots().0.join("::");
            }
            if !self.eat(&TokenKind::Colon) {
                self.error_here("E-SYN-111", "expected `:` after `implement` head");
                return None;
            }
            let suite = self.parse_suite()?;
            return Some(self.stmt(
                start,
                StmtKind::Section(Section {
                    name: "implement".into(),
                    generic: Some(format!("{}::{}", segments.join("::"), target)),
                    args: None,
                    suite,
                    source: start.cover(self.last_span()),
                    head_source: start.cover(self.last_span()),
                }),
            ));
        }

        // ident `:` section / key-value / field declaration
        if matches!(self.peek_at(1), TokenKind::Colon) {
            match self.peek_at(2).clone() {
                TokenKind::Newline | TokenKind::Indent | TokenKind::Eof => {
                    self.advance(); // name
                    self.advance(); // :
                    let suite = self.parse_suite()?;
                    return Some(self.stmt(
                        start,
                        StmtKind::Section(Section {
                            name,
                            generic: None,
                            args: None,
                            suite,
                            source: start.cover(self.last_span()),
                            head_source: start.cover(self.last_span()),
                        }),
                    ));
                }
                TokenKind::Str(_) | TokenKind::Int(_) | TokenKind::Float(_) => {
                    self.advance(); // name
                    self.advance(); // :
                    self.skip_assignment_layout();
                    let value = self.parse_expr()?;
                    return Some(self.stmt(
                        start,
                        StmtKind::Command {
                            head: vec![name],
                            argument: Some(CommandArgument::Expr(value)),
                        },
                    ));
                }
                _ => {
                    self.advance(); // name
                    self.advance(); // :
                    if let Some(ty) = self.parse_type_expr() {
                        let default = if self.eat(&TokenKind::Eq) {
                            self.skip_assignment_layout();
                            self.parse_expr()
                        } else {
                            None
                        };
                        return Some(self.stmt(
                            start,
                            StmtKind::FieldDecl {
                                visibility: None,
                                name,
                                ty,
                                default,
                            },
                        ));
                    }
                    self.error_here("E-SYN-101", "expected a type after `:`");
                    return None;
                }
            }
        }

        // assignments with indexed targets: `norm[b, t] = ...` — but
        // `minimize [a, b]` / `order [x, y]` are commands with a list
        // argument, so a `[` without `=` falls through to the command tail.
        if matches!(self.peek_at(1), TokenKind::LBracket) {
            let save = self.pos;
            self.advance(); // name
            if let Some(indices) = self.parse_index_list() {
                if self.eat(&TokenKind::Eq) {
                    self.skip_assignment_layout();
                    let value = self.parse_expr()?;
                    return Some(self.stmt(
                        start,
                        StmtKind::Assign {
                            target: Place {
                                segments: vec![name],
                                indices,
                                source: start.cover(self.last_span()),
                            },
                            value,
                        },
                    ));
                }
            }
            self.pos = save;
        }

        // Dotted section head: `core.policy:` (V6 `lower declaration:`).
        // If it is not a section, fall through to the shared
        // segments handling (does not double-consume).
        if matches!(self.peek_at(1), TokenKind::Dot | TokenKind::PathSep) {
            let (segments, via_dot) = self.collect_segments_with_dots();
            if self.peek() == &TokenKind::Colon {
                self.advance();
                let suite = self.parse_suite()?;
                return Some(self.stmt(
                    start,
                    StmtKind::Section(Section {
                        name: segments.join("."),
                        generic: None,
                        args: None,
                        suite,
                        source: start.cover(self.last_span()),
                        head_source: start.cover(self.last_span()),
                    }),
                ));
            }
            if segments.is_empty() {
                self.error_here("E-SYN-110", "expected an expression");
                return None;
            }
            return self.finish_segments_statement(start, segments, via_dot);
        }

        // general: spaced idents, dotted places, commands
        let (segments, via_dot) = self.collect_segments_with_dots();
        if segments.is_empty() {
            self.error_here("E-SYN-110", "expected an expression");
            return None;
        }
        self.finish_segments_statement(start, segments, via_dot)
    }

    /// Shared tail for collected segment runs: `name = value` assignment,
    /// multi-word `head = value` command, or plain command.
    fn finish_segments_statement(
        &mut self,
        start: Span,
        segments: Vec<String>,
        via_dot: bool,
    ) -> Option<Stmt> {
        if self.eat(&TokenKind::Eq) {
            self.skip_assignment_layout();
            let value = self.parse_expr()?;
            if segments.len() == 1 || via_dot {
                return Some(self.stmt(
                    start,
                    StmtKind::Assign {
                        target: Place {
                            segments,
                            indices: vec![],
                            source: start.cover(self.last_span()),
                        },
                        value,
                    },
                ));
            }
            // `budget iterations = N` command with value
            return Some(self.stmt(
                start,
                StmtKind::Command {
                    head: segments,
                    argument: Some(CommandArgument::Expr(value)),
                },
            ));
        }
        let (head, argument) = self.parse_command_tail(segments)?;
        Some(self.stmt(start, StmtKind::Command { head, argument }))
    }

    /// After an expression, accept an equation tail: `= rhs` on the same
    /// line or on an indented continuation line (`mass * derivative(v)\n
    ///     = -(...)`). Returns `None` for a plain expression statement.
    fn parse_equation_tail(&mut self, left: &Expr, start: Span) -> Option<Stmt> {
        if self.peek() == &TokenKind::Eq {
            self.advance();
            self.skip_assignment_layout();
            let right = self.parse_expr()?;
            return Some(self.stmt(
                start,
                StmtKind::Equation {
                    left: left.clone(),
                    right,
                },
            ));
        }
        if self.peek() == &TokenKind::Newline {
            let save = self.pos;
            self.skip_newlines();
            let indented = matches!(self.peek(), TokenKind::Indent);
            if indented {
                self.advance();
            }
            // Same-level or deeper continuation: `mass * derivative(v)`
            // newline `= -(...)`. No statement begins with `=`, so a
            // leading `=` is unambiguously an equation continuation.
            if self.peek() == &TokenKind::Eq {
                self.advance();
                self.skip_assignment_layout();
                let right = self.parse_expr()?;
                if indented {
                    // The continuation line added a temporary indent;
                    // balance it with exactly one Dedent after the line
                    // (the suite's own closing Dedent stays for the
                    // enclosing block).
                    if self.peek() == &TokenKind::Newline {
                        self.skip_newlines();
                    }
                    if matches!(self.peek(), TokenKind::Dedent) {
                        self.advance();
                    }
                }
                return Some(self.stmt(
                    start,
                    StmtKind::Equation {
                        left: left.clone(),
                        right,
                    },
                ));
            }
            self.pos = save;
        }
        None
    }


    /// For `<name>:` section heads: scan ahead for a matching `>` that is
    /// followed by `:` or `(` (a section head), not by another comparison
    /// operand (`a < b < c` is an expression).
    fn lookahead_matches_lt_angle_head(&self) -> bool {
        let max = self.tokens.len().saturating_sub(1);
        let mut depth = 0_u32;
        let mut index = self.pos + 1;
        while index < max {
            match &self.tokens[index].kind {
                TokenKind::Lt => depth += 1,
                TokenKind::Gt => {
                    if depth == 0 {
                        return false;
                    }
                    depth -= 1;
                    if depth == 0 {
                        return matches!(
                            self.tokens.get(index + 1).map(|t| &t.kind),
                            Some(TokenKind::Colon | TokenKind::LParen)
                        );
                    }
                }
                TokenKind::Newline | TokenKind::Eof => return false,
                _ => {}
            }
            index += 1;
        }
        false
    }

    /// For dotted/`::`-led statements, decide whether the remainder is an
    /// expression (`candidate.reuse_probability ^ alpha`), a section head
    /// (`core.policy:`), or a command / assignment.
    fn dotted_continues_expression(&self) -> bool {
        let max = self.tokens.len().saturating_sub(1);
        let mut index = self.pos + 1;
        while index < max {
            match &self.tokens[index].kind {
                TokenKind::Dot | TokenKind::PathSep | TokenKind::Ident(_) => {}
                TokenKind::Star
                | TokenKind::Slash
                | TokenKind::Plus
                | TokenKind::Minus
                | TokenKind::Caret
                | TokenKind::Le
                | TokenKind::Ge
                | TokenKind::Lt
                | TokenKind::Gt
                | TokenKind::EqEq
                | TokenKind::NotEq
                | TokenKind::LParen
                | TokenKind::Eq => return true,
                _ => return false,
            }
            index += 1;
        }
        false
    }

    /// Scan a candidate `name (...)` to see whether the parens contain a
    /// parameter list (`ident : type`) or `->` follows the closing paren.
    fn looks_like_params_header(&mut self) -> bool {
        // A `name(arg-list)` is a function declaration when the parens hold
        // a typed parameter (`ident : Type`) — or when `->` follows the
        // closing paren (`define score(candidate) -> Real:`). A bare call
        // (`solve minimize(dot(...))`) is an expression.
        let mut depth: u32 = 0;
        let mut index = 1;
        let mut saw_typed = false;
        let max = self.tokens.len().saturating_sub(1);
        while index < max {
            let absolute = (self.pos + index).min(max);
            match &self.tokens[absolute].kind {
                TokenKind::LParen | TokenKind::LBracket => depth += 1,
                TokenKind::RParen | TokenKind::RBracket => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        let after = self.tokens.get(absolute + 1).map(|t| &t.kind);
                        if saw_typed {
                            return matches!(after, Some(TokenKind::Arrow | TokenKind::Colon))
                                || after == Some(&TokenKind::Newline);
                        }
                        return matches!(after, Some(TokenKind::Arrow));
                    }
                }
                TokenKind::Colon => saw_typed = true,
                TokenKind::Newline | TokenKind::Eof if depth == 0 => return false,
                _ => {}
            }
            index += 1;
        }
        false
    }

    fn parse_index_list(&mut self) -> Option<Vec<Expr>> {
        if !self.eat(&TokenKind::LBracket) {
            return None;
        }
        let mut indices = Vec::new();
        while !matches!(self.peek(), TokenKind::RBracket | TokenKind::Eof) {
            if self.eat(&TokenKind::Comma) {
                continue;
            }
            indices.push(self.parse_expr()?);
        }
        if !self.eat(&TokenKind::RBracket) {
            self.error_here("E-SYN-102", "expected `]` to close indices");
        }
        Some(indices)
    }

    /// Collect space-separated identifiers (`extends model`, `budget iterations`).
    fn collect_spaced_idents(&mut self) -> Vec<String> {
        let mut segments = Vec::new();
        while let TokenKind::Ident(next) = self.peek().clone() {
            segments.push(next);
            self.advance();
        }
        segments
    }

    /// Collect identifiers optionally joined by `.` or `::`. Returns
    /// segments and whether a dot/sep joined any of them. An identifier
    /// that opens generic type arguments (`Tensor<...>`), a comparison, or
    /// a call is left unconsumed so the command tail can parse it as an
    /// argument.
    fn collect_segments_with_dots(&mut self) -> (Vec<String>, bool) {
        let mut segments = Vec::new();
        let mut via_dot = false;
        if let TokenKind::Ident(first) = self.peek().clone() {
            segments.push(first);
            self.advance();
        }
        loop {
            if let TokenKind::Ident(next) = self.peek().clone() {
                // Stop before operators, comparisons, generics, calls, or a
                // following `<` — those start an argument expression
                // (`when x >= 0`, `error max_absolute <= 2e-8`,
                // `Tensor<Float32, [D]>`).
                if matches!(
                    self.peek_at(1),
                    TokenKind::Lt
                        | TokenKind::Le
                        | TokenKind::Ge
                        | TokenKind::Gt
                        | TokenKind::EqEq
                        | TokenKind::NotEq
                        | TokenKind::Plus
                        | TokenKind::Minus
                        | TokenKind::Star
                        | TokenKind::Slash
                        | TokenKind::Caret
                        | TokenKind::LParen
                        | TokenKind::LBracket
                        | TokenKind::Eq
                ) {
                    break;
                }
                segments.push(next);
                self.advance();
                continue;
            }
            if matches!(self.peek(), TokenKind::Dot | TokenKind::PathSep)
                && matches!(self.peek_at(1), TokenKind::Ident(_))
            {
                self.advance();
                if let TokenKind::Ident(next) = self.peek().clone() {
                    segments.push(next);
                    via_dot = true;
                    self.advance();
                    continue;
                }
            }
            break;
        }
        (segments, via_dot)
    }

    /// Parse the remainder of a command after its head words.
    fn parse_command_tail(
        &mut self,
        mut head: Vec<String>,
    ) -> Option<(Vec<String>, Option<CommandArgument>)> {
        loop {
            match self.peek().clone() {
                TokenKind::Ident(next) => {
                    // Continue collecting head words when followed by
                    // another word, a keyword connector, or end-of-line
                    // (`public constructor new`, `system with explicit`,
                    // `compile score for rust.library`). A dotted path,
                    // parenthesized expression, comparison, or generic
                    // type argument starts an expression, so the word is
                    // left unconsumed there.
                    if matches!(
                        self.peek_at(1),
                        TokenKind::Ident(_)
                            | TokenKind::Keyword(
                                Keyword::SelfKw
                                    | Keyword::Against
                                    | Keyword::Where
                                    | Keyword::Over
                                    | Keyword::With
                                    | Keyword::For
                                    | Keyword::On
                                    | Keyword::At
                            )
                            | TokenKind::Newline
                            | TokenKind::Dedent
                            | TokenKind::Eof
                    ) {
                        head.push(next);
                        self.advance();
                    } else {
                        break;
                    }
                }
                TokenKind::Keyword(Keyword::SelfKw) => {
                    head.push("Self".into());
                    self.advance();
                }
                TokenKind::Keyword(
                    Keyword::Against
                    | Keyword::Where
                    | Keyword::Over
                    | Keyword::With
                    | Keyword::For
                    | Keyword::On
                    | Keyword::At
                    | Keyword::Wrt
                    | Keyword::Package,
                ) => {
                    let word = match self.peek() {
                        TokenKind::Keyword(k) => k.spelling().to_string(),
                        _ => String::new(),
                    };
                    head.push(word);
                    self.advance();
                }
                _ => break,
            }
        }
        // `numeric strict-f64` / `safety forbid-unsafe`: join the dashed
        // word into the head. It arrives as `-` then ident when the first
        // word was collected, or as ident `-` ident (head collection stops
        // before the dash now).
        if head.first().is_some_and(|h| h == "numeric" || h == "safety") {
            if matches!(self.peek(), TokenKind::Minus)
                && matches!(self.peek_at(1), TokenKind::Ident(_))
            {
                self.advance(); // -
                if let TokenKind::Ident(tail) = self.peek().clone() {
                    let joined = format!("{}-{tail}", head.pop().unwrap_or_default());
                    head.push(joined);
                    self.advance();
                }
            } else if let TokenKind::Ident(word) = self.peek().clone() {
                if matches!(self.peek_at(1), TokenKind::Minus)
                    && matches!(self.peek_at(2), TokenKind::Ident(_))
                {
                    self.advance(); // strict
                    self.advance(); // -
                    if let TokenKind::Ident(tail) = self.peek().clone() {
                        head.push(format!("{word}-{tail}"));
                        self.advance();
                    }
                }
            }
        }
        if self.at_line_end() {
            return Some((head, None));
        }
        if self.eat(&TokenKind::Eq) {
            self.skip_assignment_layout();
            let value = self.parse_expr()?;
            return Some((head, Some(CommandArgument::Expr(value))));
        }
        // `define y = expr` / `method score = score`: a trailing
        // `name = value` argument after the head words.
        if matches!(self.peek(), TokenKind::Ident(_))
            && matches!(self.peek_at(1), TokenKind::Eq)
        {
            let TokenKind::Ident(name) = self.peek().clone() else {
                unreachable!()
            };
            self.advance();
            self.advance();
            self.skip_assignment_layout();
            let value = self.parse_expr()?;
            return Some((head, Some(CommandArgument::Assignment { name, value })));
        }
        // `representation Tensor<Float32, [D]>`: an identifier with generic
        // type arguments becomes a path expression carrying typed generics.
        if matches!(self.peek(), TokenKind::Ident(_))
            && matches!(self.peek_at(1), TokenKind::Lt)
            && self.lookahead_has_matching_gt()
        {
            let ty = self.parse_type_expr()?;
            let (segments, generics) = if let TypeKind::Path {
                segments,
                generic_args,
            } = ty.kind
            {
                (segments, generic_args)
            } else {
                (Vec::new(), Vec::new())
            };
            return Some((
                head,
                Some(CommandArgument::Expr(Expr {
                    kind: ExprKind::Path {
                        segments,
                        generics: if generics.is_empty() {
                            None
                        } else {
                            Some(generics)
                        },
                    },
                    source: ty.source,
                })),
            ));
        }
        let argument = if matches!(self.peek(), TokenKind::LBracket) {
            let list = self.parse_list_literal()?;
            Some(CommandArgument::List(list))
        } else {
            let expr = match self.peek() {
                TokenKind::Keyword(Keyword::Where) => {
                    self.advance();
                    self.parse_expr()
                }
                _ => self.parse_expr(),
            }?;
            Some(CommandArgument::Expr(expr))
        };
        Some((head, argument))
    }

    /// Scan ahead for a `>` that closes a `<` group before end of line
    /// (used to tell generic type arguments `Tensor<Float32, [D]>` from
    /// comparisons `confidence < 0.99`).
    fn lookahead_has_matching_gt(&mut self) -> bool {
        let mut depth = 0_u32;
        let max = self.tokens.len().saturating_sub(1);
        let mut index = self.pos;
        while index < max {
            match &self.tokens[index].kind {
                TokenKind::Newline | TokenKind::Eof => return false,
                TokenKind::Lt => depth += 1,
                TokenKind::Gt => {
                    if depth == 0 {
                        return false;
                    }
                    depth -= 1;
                    if depth == 0 {
                        return true;
                    }
                }
                _ => {}
            }
            index += 1;
        }
        false
    }

    fn parse_arguments(&mut self) -> Option<Vec<Argument>> {
        if !self.eat(&TokenKind::LParen) {
            return None;
        }
        let mut args = Vec::new();
        while !matches!(self.peek(), TokenKind::RParen | TokenKind::Eof) {
            if self.eat(&TokenKind::Comma) {
                continue;
            }
            let start = self.current_span();
            if let TokenKind::Ident(name) = self.peek().clone() {
                if matches!(self.peek_at(1), TokenKind::Colon) {
                    self.advance();
                    self.advance();
                    if let Some(ty) = self.parse_type_expr() {
                        args.push(Argument {
                            name: Some(name),
                            value: ArgumentValue::Type(ty),
                            source: start.cover(self.last_span()),
                        });
                        continue;
                    }
                    return None;
                }
            }
            let expr = self.parse_expr()?;
            args.push(Argument {
                name: None,
                value: ArgumentValue::Expr(expr),
                source: start.cover(self.last_span()),
            });
        }
        self.eat(&TokenKind::RParen);
        Some(args)
    }

    // ---- types ---------------------------------------------------------

    fn parse_type_expr(&mut self) -> Option<TypeExpr> {
        let start = self.current_span();
        let mut items = vec![self.parse_type_primary()?];
        while matches!(self.peek(), TokenKind::Star | TokenKind::Slash) {
            self.advance();
            items.push(self.parse_type_primary()?);
        }
        if items.len() == 1 {
            Some(items.pop().unwrap())
        } else {
            Some(TypeExpr {
                kind: TypeKind::Product(items),
                source: start.cover(self.last_span()),
            })
        }
    }

    fn parse_type_primary(&mut self) -> Option<TypeExpr> {
        let start = self.current_span();
        match self.peek().clone() {
            TokenKind::Amp => {
                self.advance();
                let inner = self.parse_type_primary()?;
                Some(TypeExpr {
                    kind: TypeKind::Ref(Box::new(inner)),
                    source: start.cover(self.last_span()),
                })
            }
            TokenKind::LParen => {
                self.advance();
                let mut items = Vec::new();
                while !matches!(self.peek(), TokenKind::RParen | TokenKind::Eof) {
                    if self.eat(&TokenKind::Comma) {
                        continue;
                    }
                    items.push(self.parse_type_expr()?);
                }
                self.eat(&TokenKind::RParen);
                Some(TypeExpr {
                    kind: TypeKind::Tuple(items),
                    source: start.cover(self.last_span()),
                })
            }
            TokenKind::LBracket => {
                self.advance();
                let mut items = Vec::new();
                while !matches!(self.peek(), TokenKind::RBracket | TokenKind::Eof) {
                    if self.eat(&TokenKind::Comma) {
                        continue;
                    }
                    items.push(self.parse_type_expr()?);
                }
                self.eat(&TokenKind::RBracket);
                Some(TypeExpr {
                    kind: TypeKind::List(items),
                    source: start.cover(self.last_span()),
                })
            }
            TokenKind::Ident(_) | TokenKind::Keyword(Keyword::SelfKw | Keyword::Fn) => {
                let mut segments = Vec::new();
                match self.peek().clone() {
                    TokenKind::Ident(segment) => {
                        segments.push(segment);
                        self.advance();
                    }
                    TokenKind::Keyword(Keyword::SelfKw) => {
                        segments.push("Self".into());
                        self.advance();
                    }
                    _ => {
                        // `fn` type: fn(params) -> T (parsed and discarded)
                        self.advance();
                        let _ = self.parse_params_after_name();
                        if self.eat(&TokenKind::Arrow) {
                            let _ = self.parse_type_expr();
                        }
                        return Some(TypeExpr {
                            kind: TypeKind::Path {
                                segments: vec!["fn".into()],
                                generic_args: vec![],
                            },
                            source: start.cover(self.last_span()),
                        });
                    }
                }
                while matches!(self.peek(), TokenKind::PathSep) {
                    self.advance();
                    match self.peek().clone() {
                        TokenKind::Ident(segment) => {
                            segments.push(segment);
                            self.advance();
                        }
                        TokenKind::Star => {
                            self.advance();
                        }
                        _ => break,
                    }
                }
                let mut generic_args = Vec::new();
                if matches!(self.peek(), TokenKind::Lt) {
                    self.advance();
                    while !matches!(self.peek(), TokenKind::Gt | TokenKind::Eof) {
                        if self.eat(&TokenKind::Comma) {
                            continue;
                        }
                        if let Some(arg) = self.parse_type_expr() {
                            generic_args.push(arg);
                        } else {
                            break;
                        }
                    }
                    if !self.eat(&TokenKind::Gt) {
                        self.error_here("E-SYN-102", "expected `>` to close type arguments");
                    }
                }
                Some(TypeExpr {
                    kind: TypeKind::Path {
                        segments,
                        generic_args,
                    },
                    source: start.cover(self.last_span()),
                })
            }
            other => {
                self.error_here(
                    "E-SYN-101",
                    format!("expected a type, found {}", other.describe()),
                );
                None
            }
        }
    }

    // ---- expressions ---------------------------------------------------

    fn parse_expr(&mut self) -> Option<Expr> {
        self.parse_expr_depth(0)
    }

    fn parse_expr_depth(&mut self, depth: usize) -> Option<Expr> {
        if depth > MAX_EXPR_DEPTH {
            self.error_here("E-SYN-106", "expression nesting limit exceeded");
            return None;
        }
        let mut expr = self.parse_or(depth)?;
        // postfix clauses: `derivative x wrt y`, `temperature at time.start`,
        // `temperature on boundary(Ω)`, `choice if condition`
        loop {
            match self.peek().clone() {
                TokenKind::Keyword(Keyword::Wrt) => {
                    self.advance();
                    if let ExprKind::Derivative { wrt: None, .. } = &expr.kind {
                        let mut items = vec![self.parse_expr_depth(depth + 1)?];
                        while self.eat(&TokenKind::Comma) {
                            items.push(self.parse_expr_depth(depth + 1)?);
                        }
                        let start = expr.source;
                        expr = Expr {
                            kind: ExprKind::Derivative {
                                value: Box::new(expr),
                                wrt: Some(items),
                            },
                            source: start.cover(self.last_span()),
                        };
                    } else {
                        break;
                    }
                }
                TokenKind::Keyword(Keyword::At) if depth > 0 => {
                    self.advance();
                    let location = self.parse_expr_depth(depth + 1)?;
                    let start = expr.source;
                    expr = Expr {
                        kind: ExprKind::At {
                            value: Box::new(expr),
                            location: Box::new(location),
                        },
                        source: start.cover(self.last_span()),
                    };
                }
                TokenKind::Keyword(Keyword::On) if depth > 0 => {
                    self.advance();
                    let location = self.parse_expr_depth(depth + 1)?;
                    let start = expr.source;
                    expr = Expr {
                        kind: ExprKind::On {
                            value: Box::new(expr),
                            location: Box::new(location),
                        },
                        source: start.cover(self.last_span()),
                    };
                }
                TokenKind::Keyword(Keyword::If) if depth > 0 => {
                    self.advance();
                    let condition = self.parse_loose_expr()?;
                    let start = expr.source;
                    expr = Expr {
                        kind: ExprKind::Conditioned {
                            value: Box::new(expr),
                            condition: Box::new(condition),
                        },
                        source: start.cover(self.last_span()),
                    };
                }
                _ => break,
            }
        }
        Some(expr)
    }

    fn parse_or(&mut self, depth: usize) -> Option<Expr> {
        let mut left = self.parse_and(depth)?;
        loop {
            if self.skip_continuation_lines() {
                continue;
            }
            if !matches!(
                self.peek(),
                TokenKind::Keyword(Keyword::Or) | TokenKind::Pipe
            ) {
                break;
            }
            self.advance();
            if self.skip_continuation_lines() {
                // operator-first continuation
            }
            let right = self.parse_and(depth)?;
            let span = left.source.cover(right.source);
            left = Expr {
                kind: ExprKind::Binary {
                    op: BinaryOp::Or,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                source: span,
            };
        }
        Some(left)
    }

    fn parse_and(&mut self, depth: usize) -> Option<Expr> {
        let mut left = self.parse_comparison(depth)?;
        loop {
            if self.skip_continuation_lines() {
                continue;
            }
            if !matches!(
                self.peek(),
                TokenKind::Keyword(Keyword::And) | TokenKind::Amp
            ) {
                break;
            }
            self.advance();
            if self.skip_continuation_lines() {
                // operator-first continuation
            }
            let right = self.parse_comparison(depth)?;
            let span = left.source.cover(right.source);
            left = Expr {
                kind: ExprKind::Binary {
                    op: BinaryOp::And,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                source: span,
            };
        }
        Some(left)
    }

    fn parse_comparison(&mut self, depth: usize) -> Option<Expr> {
        let first = self.parse_additive(depth)?;
        let mut prev = first;
        let mut acc: Option<Expr> = None;
        loop {
            if self.skip_continuation_lines() {
                continue;
            }
            let Some(op) = comparison_operator(self.peek()) else {
                break;
            };
            self.advance();
            if self.skip_continuation_lines() {
                // operator-first continuation
            }
            let right = self.parse_additive(depth)?;
            let compar = Expr {
                kind: ExprKind::Binary {
                    op,
                    left: Box::new(prev.clone()),
                    right: Box::new(right.clone()),
                },
                source: prev.source.cover(right.source),
            };
            acc = Some(match acc {
                None => compar,
                Some(prior) => {
                    let span = prior.source.cover(compar.source);
                    Expr {
                        kind: ExprKind::Binary {
                            op: BinaryOp::And,
                            left: Box::new(prior),
                            right: Box::new(compar),
                        },
                        source: span,
                    }
                }
            });
            prev = right;
        }
        Some(acc.unwrap_or(prev))
    }

    fn parse_additive(&mut self, depth: usize) -> Option<Expr> {
        let mut left = self.parse_multiplicative(depth)?;
        loop {
            if self.skip_continuation_lines() {
                continue;
            }
            let op = match self.peek() {
                TokenKind::Plus => BinaryOp::Add,
                TokenKind::Minus => BinaryOp::Sub,
                _ => break,
            };
            self.advance();
            if self.skip_continuation_lines() {
                // operator-first continuation
            }
            let right = self.parse_multiplicative(depth)?;
            let span = left.source.cover(right.source);
            left = Expr {
                kind: ExprKind::Binary {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                source: span,
            };
        }
        Some(left)
    }

    fn parse_multiplicative(&mut self, depth: usize) -> Option<Expr> {
        let mut left = self.parse_unary(depth)?;
        loop {
            if self.skip_continuation_lines() {
                continue;
            }
            let op = match self.peek() {
                TokenKind::Star => BinaryOp::Mul,
                TokenKind::Slash => BinaryOp::Div,
                _ => break,
            };
            self.advance();
            if self.skip_continuation_lines() {
                // operator-first continuation
            }
            let right = self.parse_unary(depth)?;
            let span = left.source.cover(right.source);
            left = Expr {
                kind: ExprKind::Binary {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                source: span,
            };
        }
        Some(left)
    }

    fn parse_unary(&mut self, depth: usize) -> Option<Expr> {
        let start = self.current_span();
        match self.peek() {
            TokenKind::Minus => {
                self.advance();
                let value = self.parse_unary(depth)?;
                Some(Expr {
                    kind: ExprKind::Unary {
                        op: UnaryOp::Neg,
                        value: Box::new(value),
                    },
                    source: start.cover(self.last_span()),
                })
            }
            TokenKind::Plus => {
                self.advance();
                let value = self.parse_unary(depth)?;
                Some(Expr {
                    kind: ExprKind::Unary {
                        op: UnaryOp::Pos,
                        value: Box::new(value),
                    },
                    source: start.cover(self.last_span()),
                })
            }
            TokenKind::Keyword(Keyword::Not) => {
                self.advance();
                let value = self.parse_unary(depth)?;
                Some(Expr {
                    kind: ExprKind::Unary {
                        op: UnaryOp::Not,
                        value: Box::new(value),
                    },
                    source: start.cover(self.last_span()),
                })
            }
            _ => self.parse_power(depth),
        }
    }

    fn parse_power(&mut self, depth: usize) -> Option<Expr> {
        let left = self.parse_postfix(depth)?;
        let is_pow = matches!(self.peek(), TokenKind::Caret)
            || (matches!(self.peek(), TokenKind::Star)
                && matches!(self.peek_at(1), TokenKind::Star));
        if !is_pow {
            return Some(left);
        }
        self.advance();
        if matches!(self.peek(), TokenKind::Star) {
            self.advance();
        }
        let right = self.parse_unary(depth)?;
        let span = left.source.cover(right.source);
        Some(Expr {
            kind: ExprKind::Binary {
                op: BinaryOp::Pow,
                left: Box::new(left),
                right: Box::new(right),
            },
            source: span,
        })
    }

    fn parse_postfix(&mut self, depth: usize) -> Option<Expr> {
        let mut value = self.parse_primary(depth)?;
        loop {
            match self.peek() {
                TokenKind::LParen => {
                    let args = self.parse_call_args()?;
                    let span = value.source.cover(self.last_span());
                    value = Expr {
                        kind: ExprKind::Call {
                            function: Box::new(value),
                            args,
                        },
                        source: span,
                    };
                }
                TokenKind::LBracket => {
                    self.advance();
                    let mut indices = Vec::new();
                    while !matches!(self.peek(), TokenKind::RBracket | TokenKind::Eof) {
                        if self.eat(&TokenKind::Comma) {
                            continue;
                        }
                        indices.push(self.parse_expr_depth(depth + 1)?);
                    }
                    if !self.eat(&TokenKind::RBracket) {
                        self.error_here("E-SYN-102", "expected `]` to close indices");
                    }
                    let span = value.source.cover(self.last_span());
                    value = Expr {
                        kind: ExprKind::Index {
                            value: Box::new(value),
                            indices,
                        },
                        source: span,
                    };
                }
                TokenKind::Dot => {
                    self.advance();
                    match self.peek().clone() {
                        TokenKind::Ident(field) => {
                            self.advance();
                            let span = value.source.cover(self.last_span());
                            value = match &value.kind {
                                ExprKind::Path {
                                    segments,
                                    generics: None,
                                } => {
                                    let mut segments = segments.clone();
                                    segments.push(field);
                                    Expr {
                                        kind: ExprKind::Path {
                                            segments,
                                            generics: None,
                                        },
                                        source: span,
                                    }
                                }
                                _ => Expr {
                                    kind: ExprKind::Call {
                                        function: Box::new(Expr {
                                            kind: ExprKind::Path {
                                                segments: vec![field],
                                                generics: None,
                                            },
                                            source: span,
                                        }),
                                        args: vec![value.clone()],
                                    },
                                    source: span,
                                },
                            };
                        }
                        other => {
                            self.error_here(
                                "E-SYN-101",
                                format!(
                                    "expected a field name after `.`, found {}",
                                    other.describe()
                                ),
                            );
                            break;
                        }
                    }
                }
                TokenKind::DotDot | TokenKind::DotDotEq => {
                    let inclusive = self.peek() == &TokenKind::DotDotEq;
                    self.advance();
                    let end = if self.at_line_end()
                        || matches!(self.peek(), TokenKind::Comma | TokenKind::RParen)
                    {
                        None
                    } else {
                        Some(Box::new(self.parse_expr_depth(depth + 1)?))
                    };
                    let span = value.source.cover(self.last_span());
                    value = Expr {
                        kind: ExprKind::Range {
                            start: Some(Box::new(value)),
                            end,
                            inclusive,
                        },
                        source: span,
                    };
                    break;
                }
                _ => break,
            }
        }
        // Quantity literal: numeric literal followed by a unit identifier.
        if let TokenKind::Ident(unit) = self.peek().clone() {
            if matches!(&value.kind, ExprKind::Int(_) | ExprKind::Float(_)) {
                self.advance();
                let source = value.source.cover(self.last_span());
                value = Expr {
                    kind: ExprKind::Quantity {
                        value: Box::new(value),
                        unit: vec![unit],
                    },
                    source,
                };
            }
        }
        Some(value)
    }

    fn parse_call_args(&mut self) -> Option<Vec<Expr>> {
        if !self.eat(&TokenKind::LParen) {
            return None;
        }
        let mut args = Vec::new();
        while !matches!(self.peek(), TokenKind::RParen | TokenKind::Eof) {
            if self.eat(&TokenKind::Comma) {
                continue;
            }
            // keyword-style argument `round = nearest`
            if let TokenKind::Ident(_) = self.peek() {
                if matches!(self.peek_at(1), TokenKind::Eq) {
                    self.advance();
                    self.advance();
                }
            }
            args.push(self.parse_expr()?);
        }
        if !self.eat(&TokenKind::RParen) {
            self.error_here("E-SYN-102", "expected `)` to close call arguments");
        }
        Some(args)
    }

    fn parse_list_literal(&mut self) -> Option<Vec<Expr>> {
        if !self.eat(&TokenKind::LBracket) {
            return None;
        }
        let mut items = Vec::new();
        while !matches!(self.peek(), TokenKind::RBracket | TokenKind::Eof) {
            if self.eat(&TokenKind::Comma) {
                continue;
            }
            items.push(self.parse_expr()?);
        }
        if !self.eat(&TokenKind::RBracket) {
            self.error_here("E-SYN-102", "expected `]` to close list");
        }
        Some(items)
    }

    fn parse_primary(&mut self, depth: usize) -> Option<Expr> {
        let start = self.current_span();
        match self.peek().clone() {
            TokenKind::Int(text) => {
                self.advance();
                Some(Expr {
                    kind: ExprKind::Int(text),
                    source: start,
                })
            }
            TokenKind::Float(text) => {
                self.advance();
                Some(Expr {
                    kind: ExprKind::Float(text),
                    source: start,
                })
            }
            TokenKind::Str(value) => {
                self.advance();
                Some(Expr {
                    kind: ExprKind::Str(value),
                    source: start,
                })
            }
            TokenKind::Keyword(Keyword::True) => {
                self.advance();
                Some(Expr {
                    kind: ExprKind::Bool(true),
                    source: start,
                })
            }
            TokenKind::Keyword(Keyword::False) => {
                self.advance();
                Some(Expr {
                    kind: ExprKind::Bool(false),
                    source: start,
                })
            }
            TokenKind::LParen => {
                self.advance();
                let first = self.parse_expr_depth(depth + 1)?;
                if self.eat(&TokenKind::Comma) {
                    let mut items = vec![first];
                    while !matches!(self.peek(), TokenKind::RParen | TokenKind::Eof) {
                        if self.eat(&TokenKind::Comma) {
                            continue;
                        }
                        items.push(self.parse_expr_depth(depth + 1)?);
                    }
                    if !self.eat(&TokenKind::RParen) {
                        self.error_here("E-SYN-102", "expected `)` to close tuple");
                    }
                    Some(Expr {
                        kind: ExprKind::Tuple(items),
                        source: start.cover(self.last_span()),
                    })
                } else {
                    if !self.eat(&TokenKind::RParen) {
                        self.error_here("E-SYN-102", "expected `)` to close parenthesis");
                    }
                    Some(first)
                }
            }
            TokenKind::LBracket => {
                let items = self.parse_list_literal()?;
                Some(Expr {
                    kind: ExprKind::List(items),
                    source: start.cover(self.last_span()),
                })
            }
            TokenKind::Keyword(Keyword::If) => {
                // conditional expression `if c: a else: b` (rare)
                self.advance();
                let condition = self.parse_expr_depth(depth + 1)?;
                if !self.eat(&TokenKind::Colon) {
                    self.error_here("E-SYN-111", "expected `:` in conditional expression");
                    return None;
                }
                let then_value = self.parse_expr_depth(depth + 1)?;
                if !self.eat_keyword(Keyword::Else) {
                    self.error_here("E-SYN-101", "expected `else` in conditional expression");
                    return None;
                }
                if !self.eat(&TokenKind::Colon) {
                    self.error_here("E-SYN-111", "expected `:` after `else`");
                    return None;
                }
                let else_value = self.parse_expr_depth(depth + 1)?;
                Some(Expr {
                    kind: ExprKind::If {
                        condition: Box::new(condition),
                        then_value: Box::new(then_value),
                        else_value: Box::new(else_value),
                    },
                    source: start.cover(self.last_span()),
                })
            }
            TokenKind::Keyword(
                Keyword::Sum
                | Keyword::Product
                | Keyword::Integral
                | Keyword::ForAll
                | Keyword::Exists,
            ) => {
                let kind = binder_kind(self.peek());
                self.advance();
                let binders = self.parse_binders()?;
                if !self.eat(&TokenKind::Colon) {
                    self.error_here("E-SYN-111", "expected `:` after binder variables");
                    return None;
                }
                self.skip_newlines();
                if matches!(self.peek(), TokenKind::Indent) {
                    self.advance();
                }
                let body = self.parse_expr_depth(depth + 1)?;
                Some(Expr {
                    kind: ExprKind::Binder {
                        kind,
                        binders,
                        body: Box::new(body),
                    },
                    source: start.cover(self.last_span()),
                })
            }
            TokenKind::Keyword(Keyword::Derivative) => {
                self.advance();
                let value = self.parse_expr_depth(depth + 1)?;
                Some(Expr {
                    kind: ExprKind::Derivative {
                        value: Box::new(value),
                        wrt: None,
                    },
                    source: start.cover(self.last_span()),
                })
            }
            TokenKind::Ident(_) | TokenKind::Keyword(Keyword::SelfKw) => {
                let mut segments = Vec::new();
                if let TokenKind::Ident(segment) = self.peek().clone() {
                    segments.push(segment);
                    self.advance();
                } else {
                    segments.push("Self".into());
                    self.advance();
                }
                while matches!(self.peek(), TokenKind::PathSep) {
                    self.advance();
                    if let TokenKind::Ident(segment) = self.peek().clone() {
                        segments.push(segment);
                        self.advance();
                    } else {
                        break;
                    }
                }
                let mut generics = None;
                if matches!(self.peek(), TokenKind::Lt)
                    && self.lookahead_has_matching_gt()
                {
                    let save = self.pos;
                    self.advance();
                    if let Some(first_arg) = self.parse_type_expr() {
                        let mut args = vec![first_arg];
                        while self.eat(&TokenKind::Comma) {
                            let Some(arg) = self.parse_type_expr() else {
                                self.pos = save;
                                generics = None;
                                break;
                            };
                            args.push(arg);
                        }
                        if self.eat(&TokenKind::Gt) {
                            generics = Some(args);
                        } else {
                            self.pos = save;
                        }
                    } else {
                        self.pos = save;
                    }
                }
                Some(Expr {
                    kind: ExprKind::Path { segments, generics },
                    source: start.cover(self.last_span()),
                })
            }
            other => {
                self.error_here(
                    "E-SYN-110",
                    format!("expected an expression, found {}", other.describe()),
                );
                None
            }
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
