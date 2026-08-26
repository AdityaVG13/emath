use crate::token::{Keyword, TokenKind};
use crate::tree::{
    CommandArgument, Expr, ExprKind, Section, Stmt, StmtKind, TypeKind, Visibility,
};
use emath_core::Span;

use super::visibility_name;

impl super::Parser {
    pub(super) fn stmt(&self, start: Span, kind: StmtKind) -> Stmt {
        Stmt {
            kind,
            source: start.cover(self.last_span()),
        }
    }

    pub(super) fn parse_statement(&mut self) -> Option<Stmt> {
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
                    // `invariant:` section head
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
            TokenKind::Keyword(Keyword::If) => {
                // `if: Float64` / `if = 1` is a keyword used as a field or
                // binding name, not an if-statement (those need a condition).
                if matches!(self.peek_at(1), TokenKind::Colon | TokenKind::Eq) {
                    self.error_keyword_as_ident(Keyword::If);
                    self.skip_to_line_end();
                    return None;
                }
                self.parse_if_statement(start)
            }
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
            | TokenKind::Keyword(Keyword::True | Keyword::False | Keyword::Derivative | Keyword::Solve | Keyword::Minimize | Keyword::Maximize) => {
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

    /// Parse the remainder of a command after its head words.
    pub(super) fn parse_command_tail(
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
        if head
            .first()
            .is_some_and(|h| h == "numeric" || h == "safety" || h == "error")
        {
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
        if head.first().is_some_and(|word| word == "representation") {
            if let TokenKind::Ident(word) = self.peek().clone() {
                head.push(word);
                self.advance();
            }
            if matches!(self.peek(), TokenKind::Arrow) {
                self.advance();
                if let TokenKind::Ident(model) = self.peek().clone() {
                    head.push(model);
                    self.advance();
                }
                // `Float64(round = nearest, overflow = error)` is mapping
                // evidence, not a Phase 1 call; skip the parenthetical.
                if matches!(self.peek(), TokenKind::LParen) {
                    let mut depth = 0_i32;
                    while !matches!(self.peek(), TokenKind::Eof) && !self.at_line_end() {
                        match self.peek() {
                            TokenKind::LParen => depth += 1,
                            TokenKind::RParen => {
                                depth -= 1;
                                self.advance();
                                if depth == 0 {
                                    break;
                                }
                                continue;
                            }
                            _ => {}
                        }
                        self.advance();
                    }
                }
            }
            if self.at_line_end() {
                return Some((head, None));
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
        if matches!(self.peek_at(1), TokenKind::Eq)
            && let TokenKind::Ident(name) = self.peek().clone()
        {
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
    pub(super) fn lookahead_has_matching_gt(&mut self) -> bool {
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
}
