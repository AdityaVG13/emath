use crate::token::{Keyword, TokenKind};
use crate::tree::{BinaryOp, BinderKind, DerivativeKind, Expr, ExprKind, LimitDirection, UnaryOp, UnitQueryKind};
use super::{binder_kind, comparison_operator, MAX_EXPR_DEPTH};

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
                        ExprKind::Derivative { value, kind, holding, .. } => Some(Expr {
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
                        ExprKind::Optimize { value, maximize, .. } => Some(Expr {
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
                TokenKind::Ident(name) if name == "holding"
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
                    if let ExprKind::Derivative { value, wrt, kind, .. } = &expr.kind {
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

    // ---- B12: logic connectives ==> and <==>) -------------------------

    /// `<==>` — biconditional (lowest precedence, left-associative).
    fn parse_iff(&mut self, depth: usize) -> Option<Expr> {
        let mut left = self.parse_imply(depth)?;
        loop {
            if self.skip_continuation_lines() {
                continue;
            }
            if !matches!(self.peek(), TokenKind::Iff) {
                break;
            }
            self.advance();
            if self.skip_continuation_lines() {
                // operator-first continuation
            }
            let right = self.parse_imply(depth)?;
            let span = left.source.cover(right.source);
            left = Expr {
                kind: ExprKind::Binary {
                    op: BinaryOp::Iff,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                source: span,
            };
        }
        Some(left)
    }

    /// `==>` — implication (right-associative, lower than `or`).
    fn parse_imply(&mut self, depth: usize) -> Option<Expr> {
        let left = self.parse_or(depth)?;
        if self.skip_continuation_lines() {
            return self.parse_imply(depth);
        }
        if !matches!(self.peek(), TokenKind::Imply) {
            return Some(left);
        }
        self.advance();
        if self.skip_continuation_lines() {
            // operator-first continuation
        }
        let right = self.parse_imply(depth)?; // right-recursive
        let span = left.source.cover(right.source);
        Some(Expr {
            kind: ExprKind::Binary {
                op: BinaryOp::Imply,
                left: Box::new(left),
                right: Box::new(right),
            },
            source: span,
        })
    }

    fn parse_or(&mut self, depth: usize) -> Option<Expr> {
        let mut left = self.parse_and(depth)?;
        loop {
            if self.skip_continuation_lines() {
                continue;
            }
            if !matches!(
                self.peek(),
                TokenKind::Keyword(Keyword::Or)
            ) && (!matches!(self.peek(), TokenKind::Pipe) || self.suppress_pipe_or)
            {
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

    /// `unit of E` / `dimension of E` — compile-time query operators.
    /// Precedence: just above `==` (binds tighter than comparison,
    /// looser than additive). Contextual keywords: `unit` and `dimension`
    /// are identifiers that activate only when followed by `of`.
    fn parse_unit_query(&mut self, depth: usize) -> Option<Expr> {
        let start = self.current_span();
        // Check for `unit of` or `dimension of` contextual keywords.
        if let TokenKind::Ident(kw) = self.peek().clone() {
            if matches!(kw.as_str(), "unit" | "dimension") {
                if matches!(self.peek_at(1), TokenKind::Ident(id) if id == "of") {
                    let kind = if kw == "unit" {
                        UnitQueryKind::Unit
                    } else {
                        UnitQueryKind::Dimension
                    };
                    self.advance(); // consume `unit`/`dimension`
                    self.advance(); // consume `of`
                    let expr = self.parse_additive(depth)?;
                    return Some(Expr {
                        kind: ExprKind::UnitQuery {
                            kind,
                            expr: Box::new(expr),
                        },
                        source: start.cover(self.last_span()),
                    });
                }
            }
        }
        // B18: `f ~~ g` — asymptotic equivalence at comparison precedence.
        // Lowers to a limit claim in sema.
        let left = self.parse_additive(depth)?;
        if matches!(self.peek(), TokenKind::TildeTilde) {
            self.advance();
            let right = self.parse_additive(depth)?;
            let span = left.source.cover(right.source);
            return Some(Expr {
                kind: ExprKind::Binary {
                    op: BinaryOp::Asymp,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                source: span,
            });
        }
        Some(left)
    }

    fn parse_comparison(&mut self, depth: usize) -> Option<Expr> {
        let first = self.parse_unit_query(depth)?;
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
            let right = self.parse_unit_query(depth)?;
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

    pub(super) fn parse_additive(&mut self, depth: usize) -> Option<Expr> {
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
                    // C3: refuse indexing on numeric literals.  Indexing a
                    // number (e.g. `9.81 [m/s^2]`) is always a type error,
                    // and the bracket form collides with future unit-bracket
                    // syntax.  List/tuple/path primaries still accept `[]`.
                    if matches!(
                        &value.kind,
                        ExprKind::Int(_) | ExprKind::Float(_) | ExprKind::Quantity { .. }
                    ) {
                        break;
                    }
                    self.advance();
                    let mut indices = Vec::new();
                    while !matches!(self.peek(), TokenKind::RBracket | TokenKind::Eof) {
                        if self.eat(&TokenKind::Comma) {
                            continue;
                        }
                        indices.push(self.parse_index_axis(depth)?);
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

    fn parse_index_axis(&mut self, depth: usize) -> Option<Expr> {
        let start = self.current_span();
        if self.eat(&TokenKind::Colon) {
            let end = if matches!(
                self.peek(),
                TokenKind::Comma | TokenKind::RBracket | TokenKind::Eof
            ) {
                None
            } else {
                Some(Box::new(self.parse_expr_depth(depth + 1)?))
            };
            return Some(Expr {
                kind: ExprKind::Slice { start: None, end },
                source: start.cover(self.last_span()),
            });
        }
        let first = self.parse_expr_depth(depth + 1)?;
        if self.eat(&TokenKind::Colon) {
            let end = if matches!(
                self.peek(),
                TokenKind::Comma | TokenKind::RBracket | TokenKind::Eof
            ) {
                None
            } else {
                Some(Box::new(self.parse_expr_depth(depth + 1)?))
            };
            return Some(Expr {
                kind: ExprKind::Slice {
                    start: Some(Box::new(first)),
                    end,
                },
                source: start.cover(self.last_span()),
            });
        }
        Some(first)
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
            // Keyword-style argument `round = nearest`: the AST call
            // node is positional-only, so a named argument cannot be
            // represented without silently dropping its name; refuse it
            // (E-SYN-121) instead of stripping it (SURF-0011).
            if let TokenKind::Ident(name) = self.peek().clone() {
                if matches!(self.peek_at(1), TokenKind::Eq) {
                    self.error_here(
                        "E-SYN-121",
                        format!(
                            "named call argument `{name} = ...` is outside the Phase 1 subset \
                             (calls are positional-only)"
                        ),
                    );
                    self.advance();
                    self.advance();
                    args.push(self.parse_expr()?);
                    continue;
                }
            }
            args.push(self.parse_expr()?);
        }
        if !self.eat(&TokenKind::RParen) {
            self.error_here("E-SYN-102", "expected `)` to close call arguments");
        }
        Some(args)
    }

    pub(super) fn parse_list_literal(&mut self) -> Option<Vec<Expr>> {
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
                // B14: Complex literal suffix `Ni` (e.g., `2i`, `3.5i`).
                // The lexer includes `i` in the Float token text. We
                // desugar to `N * i` where `i` is the imaginary unit
                // (a named constant resolved by sema).
                if text.ends_with('i') && text.len() > 1 {
                    let coeff = &text[..text.len() - 1];
                    return Some(Expr {
                        kind: ExprKind::Binary {
                            op: BinaryOp::Mul,
                            left: Box::new(Expr {
                                kind: ExprKind::Float(coeff.to_string()),
                                source: start,
                            }),
                            right: Box::new(Expr {
                                kind: ExprKind::Path {
                                    segments: vec!["i".to_string()],
                                    generics: None,
                                },
                                source: start.cover(self.last_span()),
                            }),
                        },
                        source: start,
                    });
                }
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
                if self.peek_reduction_call() {
                    let TokenKind::Keyword(keyword) = self.peek().clone() else {
                        return None;
                    };
                    self.advance();
                    return Some(Expr {
                        kind: ExprKind::Path {
                            segments: vec![keyword.spelling().to_string()],
                            generics: None,
                        },
                        source: start,
                    });
                }
                let kind = binder_kind(self.peek());
                self.advance();
                let binders = self.parse_binders()?;
                // B02: optional `if <condition>` guard clause.
                let guard = self.parse_binder_guard();
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
                        guard,
                    },
                    source: start.cover(self.last_span()),
                })
            }
            TokenKind::Keyword(Keyword::Derivative) => {
                self.advance();
                // F5: restrict operand to postfix_expr so that
                // `derivative(v) + v` parses as `(derivative v) + v`,
                // not `derivative(v + v)`.  Parenthesised operands
                // (`derivative(v + v)`) still work because a parenthesised
                // expression is a primary_expr, hence a postfix_expr.
                let value = self.parse_postfix(depth + 1)?;
                Some(Expr {
                    kind: ExprKind::Derivative {
                        value: Box::new(value),
                        wrt: None,
                        kind: DerivativeKind::Plain,
                        holding: Vec::new(),
                    },
                    source: start.cover(self.last_span()),
                })
            }
            TokenKind::Keyword(Keyword::Solve) => {
                self.advance();
                let value = self.parse_expr_depth(depth + 1)?;
                Some(Expr {
                    kind: ExprKind::Solve {
                        value: Box::new(value),
                        wrt: None,
                    },
                    source: start.cover(self.last_span()),
                })
            }
            TokenKind::Keyword(Keyword::Minimize) | TokenKind::Keyword(Keyword::Maximize) => {
                let maximize = matches!(self.peek(), TokenKind::Keyword(Keyword::Maximize));
                self.advance();
                let value = self.parse_expr_depth(depth + 1)?;
                Some(Expr {
                    kind: ExprKind::Optimize {
                        value: Box::new(value),
                        wrt: None,
                        maximize,
                    },
                    source: start.cover(self.last_span()),
                })
            }
            TokenKind::Ident(_) | TokenKind::Keyword(Keyword::SelfKw) => {
                // B04: `limit x -> 0: f(x)` — contextual keyword for limit
                // claim. Activates only when `limit` is followed by an
                // identifier and then `->`. Otherwise `limit` is a regular
                // user identifier.
                if let TokenKind::Ident(name) = self.peek().clone() {
                    if name == "limit"
                        && matches!(self.peek_at(1), TokenKind::Ident(_))
                        && matches!(self.peek_at(2), TokenKind::Arrow)
                    {
                        self.advance(); // `limit`
                        let TokenKind::Ident(var) = self.peek().clone() else {
                            unreachable!()
                        };
                        self.advance(); // var
                        self.advance(); // `->`
                        return Some(self.parse_limit_body(
                            start,
                            var,
                            false, // is_sample = false
                            depth,
                        )?);
                    }
                    if name == "sample_limit"
                        && matches!(self.peek_at(1), TokenKind::Ident(_))
                        && matches!(self.peek_at(2), TokenKind::Arrow)
                    {
                        self.advance(); // `sample_limit`
                        let TokenKind::Ident(var) = self.peek().clone() else {
                            unreachable!()
                        };
                        self.advance(); // var
                        self.advance(); // `->`
                        return Some(self.parse_limit_body(
                            start,
                            var,
                            true, // is_sample = true
                            depth,
                        )?);
                    }
                    // B06: `series n in 0..inf: a[n]` — contextual keyword
                    // for series binder. Activates only when `series` is
                    // followed by an identifier and then `in`.
                    if name == "series"
                        && matches!(self.peek_at(1), TokenKind::Ident(_))
                        && matches!(self.peek_at(2), TokenKind::Keyword(Keyword::In))
                    {
                        self.advance(); // `series`
                        let binders = self.parse_binders()?;
                        let guard = self.parse_binder_guard();
                        if !self.eat(&TokenKind::Colon) {
                            self.error_here("E-SYN-111", "expected `:` after series binder variables");
                            return None;
                        }
                        self.skip_newlines();
                        if matches!(self.peek(), TokenKind::Indent) {
                            self.advance();
                        }
                        let body = self.parse_expr_depth(depth + 1)?;
                        return Some(Expr {
                            kind: ExprKind::Binder {
                                kind: BinderKind::Series,
                                binders,
                                body: Box::new(body),
                                guard,
                            },
                            source: start.cover(self.last_span()),
                        });
                    }
                    // U1: `cases x: | c1 => e1 | else => e2` - contextual
                    // keyword for cases expression. Activates when `cases`
                    // is followed by `:` (no subject) or by an identifier
                    // and then `:` (subject is a simple name).
                    if name == "cases" {
                        if matches!(self.peek_at(1), TokenKind::Colon) {
                            self.advance(); // `cases`
                            self.advance(); // `:`
                            return Some(self.parse_cases_body(start, None, depth)?);
                        }
                        if matches!(self.peek_at(1), TokenKind::Ident(_))
                            && matches!(self.peek_at(2), TokenKind::Colon)
                        {
                            self.advance(); // `cases`
                            let subject = self.parse_primary(depth)?;
                            self.advance(); // `:`
                            return Some(self.parse_cases_body(
                                start,
                                Some(Box::new(subject)),
                                depth,
                            )?);
                        }
                    }
                }
                // Contextual keywords for partial/total derivatives:
                // `partial(T)`, `∂(T)`, `total(T)`, `d(T)` — only when
                // followed by `(`.  Otherwise these are regular identifiers.
                if let TokenKind::Ident(name) = self.peek().clone() {
                    if matches!(self.peek_at(1), TokenKind::LParen) {
                        match name.as_str() {
                            "partial" | "\u{2202}" => {
                                self.advance();
                                let value = self.parse_postfix(depth + 1)?;
                                return Some(Expr {
                                    kind: ExprKind::Derivative {
                                        value: Box::new(value),
                                        wrt: None,
                                        kind: DerivativeKind::Partial,
                                        holding: Vec::new(),
                                    },
                                    source: start.cover(self.last_span()),
                                });
                            }
                            "total" | "d" => {
                                self.advance();
                                let value = self.parse_postfix(depth + 1)?;
                                return Some(Expr {
                                    kind: ExprKind::Derivative {
                                        value: Box::new(value),
                                        wrt: None,
                                        kind: DerivativeKind::Total,
                                        holding: Vec::new(),
                                    },
                                    source: start.cover(self.last_span()),
                                });
                            }
                            _ => {}
                        }
                    }
                }
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
                if matches!(self.peek(), TokenKind::Lt) && self.lookahead_has_matching_gt() {
                    let save = self.pos;
                    self.advance();
                    if let Some(first_arg) = self.parse_generic_arg() {
                        let mut args = vec![first_arg];
                        while self.eat(&TokenKind::Comma) {
                            let Some(arg) = self.parse_generic_arg() else {
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

    /// B04: Parse the body of a `limit x -> T[+|-]: body` or
    /// `sample_limit x -> T[+|-]: body` expression.
    ///
    /// The caller has already consumed `limit`/`sample_limit`, the
    /// variable name, and `->`.  We parse the target at the
    /// multiplicative level so that `+`/`-` before `:` is interpreted
    /// as a one-sided direction suffix, not a binary operator.  Complex
    /// targets like `a + b` require parentheses: `limit x -> (a + b): f(x)`.
    fn parse_limit_body(
        &mut self,
        start: emath_core::Span,
        var: String,
        is_sample: bool,
        depth: usize,
    ) -> Option<Expr> {
        let target = self.parse_multiplicative(depth)?;
        // One-sided suffix: `+` or `-` immediately before `:`.
        let direction = if matches!(self.peek(), TokenKind::Plus | TokenKind::Minus)
            && matches!(self.peek_at(1), TokenKind::Colon)
        {
            let dir = if matches!(self.peek(), TokenKind::Plus) {
                LimitDirection::FromAbove
            } else {
                LimitDirection::FromBelow
            };
            self.advance(); // consume `+` or `-`
            dir
        } else {
            LimitDirection::TwoSided
        };
        if !self.eat(&TokenKind::Colon) {
            self.error_here("E-SYN-111", "expected `:` after limit target");
            return None;
        }
        self.skip_newlines();
        if matches!(self.peek(), TokenKind::Indent) {
            self.advance();
        }
        let body = self.parse_expr_depth(depth + 1)?;
        let kind = if is_sample {
            ExprKind::SampleLimit {
                var,
                target: Box::new(target),
                direction,
                body: Box::new(body),
            }
        } else {
            ExprKind::Limit {
                var,
                target: Box::new(target),
                direction,
                body: Box::new(body),
            }
        };
        Some(Expr {
            kind,
            source: start.cover(self.last_span()),
        })
    }

    /// U1: Parse the body of a `cases [subject]: | c1 => e1 | else => eN`
    /// expression. The caller has already consumed `cases`, the optional
    /// subject, and `:`. Arms are delimited by `|` and use `=>` as the
    /// arm arrow. A mandatory `else` arm enforces totality at parse time.
    fn parse_cases_body(
        &mut self,
        start: emath_core::Span,
        subject: Option<Box<Expr>>,
        depth: usize,
    ) -> Option<Expr> {
        self.skip_newlines();
        if matches!(self.peek(), TokenKind::Indent) {
            self.advance();
        }
        let mut arms = Vec::new();
        let mut else_arm = None;
        // Suppress `|` as `or` so it acts as arm delimiter.
        self.suppress_pipe_or = true;
        loop {
            self.skip_newlines();
            if !self.eat(&TokenKind::Pipe) {
                self.error_here(
                    "E-SYN-110",
                    "expected `|` to start a cases arm",
                );
                return None;
            }
            self.skip_newlines();
            // Check for `else` arm.
            if matches!(self.peek(), TokenKind::Keyword(Keyword::Else)) {
                self.advance(); // `else`
                if !self.eat(&TokenKind::Arrow) {
                    self.error_here(
                        "E-SYN-101",
                        "expected `=>` after `else` in cases arm",
                    );
                    return None;
                }
                self.skip_newlines();
                let value = self.parse_expr_depth(depth + 1)?;
                else_arm = Some(Box::new(value));
                break;
            }
            // Regular arm: `| condition => value`
            let condition = self.parse_expr_depth(depth + 1)?;
            if !self.eat(&TokenKind::Arrow) {
                self.error_here(
                    "E-SYN-101",
                    "expected `=>` in cases arm",
                );
                return None;
            }
            self.skip_newlines();
            let value = self.parse_expr_depth(depth + 1)?;
            arms.push((condition, value));
        }
        // Restore `|` as `or` operator.
        self.suppress_pipe_or = false;
        let Some(else_arm) = else_arm else {
            self.error_here(
                "E-SYN-110",
                "cases expression requires a mandatory `else` arm",
            );
            return None;
        };
        if arms.is_empty() {
            self.error_here(
                "E-SYN-110",
                "cases expression requires at least one condition arm before `else`",
            );
            return None;
        }
        Some(Expr {
            kind: ExprKind::Cases {
                subject,
                arms,
                else_arm,
            },
            source: start.cover(self.last_span()),
        })
    }
}
