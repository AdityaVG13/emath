use crate::token::{Keyword, TokenKind};
use crate::tree::{BinaryOp, Expr, ExprKind, Stmt, StmtKind, Suite};
use emath_core::Span;

impl super::Parser {
    pub(super) fn parse_suite(&mut self) -> Option<Suite> {
        self.parse_suite_inner(false)
    }

    /// `example <name>:` with no indented body is a worked example, not
    /// `E-SYN-112`. Other section heads still require a block.
    pub(super) fn parse_section_suite(&mut self, section_name: &str) -> Option<Suite> {
        self.parse_suite_inner(section_name == "example")
    }

    fn parse_suite_inner(&mut self, allow_empty: bool) -> Option<Suite> {
        let start = self.current_span();
        if self.at_line_end() {
            self.finish_line();
            if !matches!(self.peek(), TokenKind::Indent) {
                if !allow_empty {
                    self.error_here("E-SYN-112", "expected an indented block");
                }
                return Some(Suite {
                    statements: Vec::new(),
                    source: start,
                });
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

    /// `require section inputs`, `require guarded`: loose path acceptance.
    pub(super) fn parse_loose_expr(&mut self) -> Option<Expr> {
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

    pub(super) fn parse_if_statement(&mut self, start: Span) -> Option<Stmt> {
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

    pub(super) fn parse_self_block(&mut self, start: Span) -> Option<Stmt> {
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
}
