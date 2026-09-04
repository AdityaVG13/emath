use super::{MAX_EXPR_DEPTH, binder_kind, comparison_operator};
use crate::token::{Keyword, NablaForm, TokenKind};
use crate::tree::{
    ApproxTolerance, BinaryOp, Binder, BinderKind, DerivativeKind, Expr, ExprKind, LimitDirection,
    NotationFixity, UnaryOp, UnitExpr, UnitQueryKind,
};
use emath_core::Span;

impl super::Parser {
    // ---- expressions ---------------------------------------------------

    pub(super) fn parse_expr(&mut self) -> Option<Expr> {
        self.parse_expr_depth(0)
    }

    fn parse_expr_depth(&mut self, depth: usize) -> Option<Expr> {
        if depth > MAX_EXPR_DEPTH {
            self.error_here("E-SYN-106", "expression nesting limit exceeded");
            return None;
        }
        let mut expr = self.parse_iff(depth)?;
        // postfix clauses: `derivative x wrt y`, `temperature at time.start`,
        // `temperature on boundary(Ω)`, `choice if condition`
        loop {
            match self.peek().clone() {
                TokenKind::Keyword(Keyword::Wrt) => {
                    // Attach `wrt` to a solver node (Derivative, Solve,
                    // Optimize) that doesn't already have one.
                    if !matches!(
                        &expr.kind,
                        ExprKind::Derivative { wrt: None, .. }
                            | ExprKind::Solve { wrt: None, .. }
                            | ExprKind::Optimize { wrt: None, .. }
                    ) {
                        break;
                    }
                    self.advance();
                    let mut items = vec![self.parse_expr_depth(depth + 1)?];
                    while self.eat(&TokenKind::Comma) {
                        items.push(self.parse_expr_depth(depth + 1)?);
                    }
                    let start = expr.source;
                    let Some(next) = (match &expr.kind {
                        ExprKind::Derivative {
                            value,
                            kind,
                            holding,
                            ..
                        } => Some(Expr {
                            kind: ExprKind::Derivative {
                                value: value.clone(),
                                wrt: Some(items),
                                kind: *kind,
                                holding: holding.clone(),
                            },
                            source: start.cover(self.last_span()),
                        }),
                        ExprKind::Solve { value, .. } => Some(Expr {
                            kind: ExprKind::Solve {
                                value: value.clone(),
                                wrt: Some(items),
                            },
                            source: start.cover(self.last_span()),
                        }),
                        ExprKind::Optimize {
                            value, maximize, ..
                        } => Some(Expr {
                            kind: ExprKind::Optimize {
                                value: value.clone(),
                                wrt: Some(items),
                                maximize: *maximize,
                            },
                            source: start.cover(self.last_span()),
                        }),
                        // Guard above admits only Derivative/Solve/Optimize.
                        _ => {
                            self.error_here(
                                "E-SYN-107",
                                "`wrt` applies only to derivative, solve, or optimize",
                            );
                            None
                        }
                    }) else {
                        return None;
                    };
                    expr = next;
                }
                // `holding` clause: `∂(H) wrt T holding p, V`
                // Contextual keyword — only matches when the current
                // expression is a Partial derivative without holding set.
                TokenKind::Ident(name)
                    if name == "holding"
                        && matches!(
                            &expr.kind,
                            ExprKind::Derivative {
                                kind: DerivativeKind::Partial,
                                holding: h,
                                ..
                            } if h.is_empty()
                        ) =>
                {
                    self.advance();
                    let mut items = vec![self.parse_expr_depth(depth + 1)?];
                    while self.eat(&TokenKind::Comma) {
                        items.push(self.parse_expr_depth(depth + 1)?);
                    }
                    let start = expr.source;
                    if let ExprKind::Derivative {
                        value, wrt, kind, ..
                    } = &expr.kind
                    {
                        expr = Expr {
                            kind: ExprKind::Derivative {
                                value: value.clone(),
                                wrt: wrt.clone(),
                                kind: *kind,
                                holding: items,
                            },
                            source: start.cover(self.last_span()),
                        };
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
