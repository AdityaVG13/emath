use crate::token::{Keyword, TokenKind};
use crate::tree::{Binder, Expr, ExprKind, Param, Stmt, StmtKind, TypeExpr, TypeKind, Visibility};
use emath_core::Span;

use super::binder_kind;

impl super::Parser {
    /// `fn name(params) [-> Ret] [:] suite`
    pub(super) fn parse_fn_statement(
        &mut self,
        start: Span,
        visibility: Option<Visibility>,
    ) -> Option<Stmt> {
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

    pub(super) fn parse_params_header(&mut self) -> Option<(String, Vec<Param>, Option<TypeExpr>)> {
        let TokenKind::Ident(name) = self.peek().clone() else {
            self.error_here("E-SYN-110", "expected a name");
            return None;
        };
        self.advance();
        // `extern operator semantic_distance<D: Nat>(...)` generics:
        // OperatorDecl carries no generic parameters and Phase 1 has no
        // generic-operator semantics. Refuse loudly (E-TYPE-112) instead of
        // parsing and discarding the generic parameter list.
        if matches!(self.peek(), TokenKind::Lt) {
            self.error_here(
                "E-TYPE-112",
                "generic extern operator declarations are outside the Phase 1 subset",
            );
            return None;
        }
        self.parse_params_after_name()
            .map(|(params, ret)| (name, params, ret))
    }

    pub(super) fn parse_params_after_name(&mut self) -> Option<(Vec<Param>, Option<TypeExpr>)> {
        self.parse_params_after_name_flag(false)
    }

    /// `allow_untyped` accepts `name` without `: Type` (method-style defines
    /// like `define score(candidate) -> Real:`), synthesizing an `Infer`
    /// marker type so the tree stays typed.
    pub(super) fn parse_params_after_name_flag(
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

    pub(super) fn parse_binder_statement(&mut self, start: Span) -> Option<Stmt> {
        if self.peek_reduction_call() {
            let expr = self.parse_expr()?;
            return Some(self.stmt(start, StmtKind::Expr(expr)));
        }
        let kind = binder_kind(self.peek());
        self.advance();
        let binders = self.parse_binders()?;
        // B02: optional `if <condition>` guard clause.
        let guard = self.parse_binder_guard();
        if self.eat(&TokenKind::Colon) {
            let suite = self.parse_suite()?;
            Some(self.stmt(
                start,
                StmtKind::BinderStmt {
                    kind,
                    binders,
                    suite,
                    guard,
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
                    guard,
                },
                source: start.cover(self.last_span()),
            };
            Some(self.stmt(start, StmtKind::Expr(expr)))
        }
    }

    pub(super) fn parse_binders(&mut self) -> Option<Vec<Binder>> {
        let mut binders = Vec::new();
        // B02: suppress postfix `if` so it's available as a guard clause
        // rather than being consumed as a conditioned expression on the
        // binder's domain (e.g. `sum i in 0..n if cond: body`).
        let prev_flag = self.suppress_postfix_if;
        self.suppress_postfix_if = true;
        loop {
            let start = self.current_span();
            let TokenKind::Ident(name) = self.peek().clone() else {
                self.error_here("E-SYN-110", "expected a binder variable name");
                self.suppress_postfix_if = prev_flag;
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
        self.suppress_postfix_if = prev_flag;
        Some(binders)
    }

    /// B02: parse the optional `if <condition>` guard clause on a binder.
    /// Returns `Some(expr)` if `if` is present, `None` otherwise.
    pub(super) fn parse_binder_guard(&mut self) -> Option<Box<Expr>> {
        if self.eat_keyword(Keyword::If) {
            self.parse_expr().map(Box::new)
        } else {
            None
        }
    }
}
