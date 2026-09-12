use super::{MAX_EXPR_DEPTH, comparison_operator};
use crate::token::{Keyword, NablaForm, TokenKind};
use crate::tree::{
    ApproxTolerance, BinaryOp, Expr, ExprKind, NotationFixity,
    UnaryOp, UnitExpr, UnitQueryKind,
};
use emath_core::Span;

impl super::Parser {
    // ---- expressions ---------------------------------------------------

    pub(super) fn parse_expr(&mut self) -> Option<Expr> {
        self.parse_expr_depth(0)
    }

    /// Domain / function-type position: `A -> B` is admitted here.
    pub(super) fn parse_domain_expr(&mut self, depth: usize) -> Option<Expr> {
        let saved = self.allow_fn_arrow;
        self.allow_fn_arrow = true;
        let expr = self.parse_expr_depth(depth + 1);
        self.allow_fn_arrow = saved;
        expr
    }

    fn parse_expr_depth(&mut self, depth: usize) -> Option<Expr> {
        if depth > MAX_EXPR_DEPTH {
            self.error_here("E-SYN-106", "expression nesting limit exceeded");
            return None;
        }
        let mut expr = self.parse_iff(depth)?;
        if self.allow_fn_arrow && matches!(self.peek(), TokenKind::Arrow) {
            self.advance();
            let right = self.parse_expr_depth(depth)?;
            let span = expr.source.cover(right.source);
            expr = Expr {
                kind: ExprKind::Call {
                    function: Box::new(Expr {
                        kind: ExprKind::Path {
                            segments: vec!["Fn".into()],
                            generics: None,
                        },
                        source: expr.source,
                    }),
                    args: vec![expr, right],
                },
                source: span,
            };
        }
        // postfix clauses: `choice if condition`
        loop {
            match self.peek().clone() {
                TokenKind::Keyword(Keyword::If) if depth > 0 && !self.suppress_postfix_if => {
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
}

mod braket;
mod forms;
mod infix;
mod literals;
mod postfix;
mod primary;
mod units;
