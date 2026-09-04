//! Postfix chain: unary, power, postfix calls/indexing, index axes, call arguments.

use super::*;

impl super::super::Parser {
    pub(super) fn parse_unary(&mut self, depth: usize) -> Option<Expr> {
        let start = self.current_span();
        // Custom-operator prefix use: `¬ a` → target(a) (notation
        // declarations; binds at the unary level, tighter than custom
        // infix, alongside `-`/`+`/`not`).
        if let TokenKind::Ident(name) = self.peek().clone() {
            let prefix = self
                .notations
                .get(&name)
                .filter(|op| op.fixity == NotationFixity::Prefix)
                .map(|op| op.target.clone());
            if let Some(target) = prefix {
                self.advance();
                let value = self.parse_unary(depth)?;
                let span = start.cover(self.last_span());
                return Some(self.notation_call(&target, vec![value], span));
            }
        }
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

    pub(super) fn parse_power(&mut self, depth: usize) -> Option<Expr> {
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

    pub(super) fn parse_postfix(&mut self, depth: usize) -> Option<Expr> {
        let mut value = self.parse_primary(depth)?;
        loop {
            match self.peek() {
                TokenKind::LParen => {
                    // C3-analog (spec 04 section 1.5): a parenthetical after a
                    // numeric literal is never a call. Attachment is lexical
                    // (`0.5012(3)` lexes as one FloatUncertainty token), so a
                    // spaced `1.50 (2)` is a syntax error at the leftover `(`,
                    // not a silently-admitted call of a number.
                    if matches!(
                        &value.kind,
                        ExprKind::Int(_)
                            | ExprKind::Float(_)
                            | ExprKind::Rational { .. }
                            | ExprKind::Quantity { .. }
                            | ExprKind::Measured { .. }
                    ) {
                        break;
                    }
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
                        ExprKind::Int(_)
                            | ExprKind::Float(_)
                            | ExprKind::Rational { .. }
                            | ExprKind::Quantity { .. }
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
                // Custom-operator postfix use: `x′` → target(x) (notation
                // declarations; binds at the postfix level, tightest).
                TokenKind::Ident(name) => {
                    let postfix = self
                        .notations
                        .get(name)
                        .filter(|op| op.fixity == NotationFixity::Postfix)
                        .map(|op| op.target.clone());
                    let Some(target) = postfix else {
                        break;
                    };
                    self.advance();
                    let span = value.source.cover(self.last_span());
                    value = self.notation_call(&target, vec![value], span);
                }
                _ => break,
            }
        }
        // Quantity literal: numeric literal followed by a unit identifier.
        // Grammar: (integer | decimal | rational_literal) whitespace path.
        if let TokenKind::Ident(unit) = self.peek().clone() {
            if matches!(
                &value.kind,
                ExprKind::Int(_) | ExprKind::Float(_) | ExprKind::Rational { .. }
            ) {
                // A registered notation glyph is an operator, not a unit:
                // leave it for `parse_notation_infix` (N4 precedence-law
                // sibling — never resolved by folding it into a literal).
                let is_notation_glyph = self.notations.contains_key(&unit);
                if !is_notation_glyph {
                    // Anti-proposal bonus (C15): no juxtaposition
                    // multiplication. `2x` is not `2 * x`; the cost of `*`
                    // is one character. Grammar requires whitespace between
                    // the numeric literal and the unit; adjacent spans
                    // (zero-byte gap) are a lexer-ambiguity refusal, never
                    // a silent quantity or product.
                    let gap = self.current_span().start.saturating_sub(value.source.end);
                    if gap == 0 {
                        self.error_here(
                            "E-SYN-101",
                            format!(
                                "juxtaposition is refused: `{unit}` binds a numeric literal with no space; write `2 * {unit}` for multiplication or `2 {unit}` for a quantity"
                            ),
                        );
                    } else {
                        self.advance();
                        let source = value.source.cover(self.last_span());
                        value = Expr {
                            kind: ExprKind::Quantity {
                                value: Box::new(value),
                                unit: UnitExpr::Base(unit),
                            },
                            source,
                        };
                    }
                }
            }
        }
        // Compound-unit bracket: `9.81 [unit m/s^2]` (F7/U4).
        // The C3 fix already broke out of the postfix loop when `[`
        // follows a numeric literal, so we handle it here.
        if matches!(
            &value.kind,
            ExprKind::Int(_)
                | ExprKind::Float(_)
                | ExprKind::Rational { .. }
                | ExprKind::Quantity { .. }
        ) {
            if matches!(self.peek(), TokenKind::LBracket) {
                if let Some(unit_expr) = self.parse_unit_bracket(depth) {
                    // Extract the inner numeric value (strip any prior unit).
                    let inner_value = match &value.kind {
                        ExprKind::Quantity { value: inner, .. } => inner.clone(),
                        _ => Box::new(value.clone()),
                    };
                    let source = value.source.cover(self.last_span());
                    value = Expr {
                        kind: ExprKind::Quantity {
                            value: inner_value,
                            unit: unit_expr,
                        },
                        source,
                    };
                }
            }
        }
        Some(value)
    }

    pub(super) fn parse_index_axis(&mut self, depth: usize) -> Option<Expr> {
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

    pub(super) fn parse_call_args(&mut self) -> Option<Vec<Expr>> {
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

    pub(in crate::parser) fn parse_list_literal(&mut self) -> Option<Vec<Expr>> {
        if !self.eat(&TokenKind::LBracket) {
            return None;
        }
        // U9: `;` splits rows (`[1, 2; 3, 4]` = 2x2 matrix spelling). A
        // comma-only list stays flat (additive — nothing reparses); rows
        // fold to nested `List` so admission's existing matrix path runs.
        let mut rows: Vec<Vec<Expr>> = vec![Vec::new()];
        while !matches!(self.peek(), TokenKind::RBracket | TokenKind::Eof) {
            if self.eat(&TokenKind::Comma) {
                continue;
            }
            if self.eat(&TokenKind::Semicolon) {
                if rows.last().is_some_and(Vec::is_empty) {
                    self.error_here("E-SYN-102", "empty row before `;` in list literal");
                    return None;
                }
                rows.push(Vec::new());
                continue;
            }
            let item = self.parse_expr()?;
            rows.last_mut().expect("row vec").push(item);
        }
        if rows.last().is_some_and(Vec::is_empty) && rows.len() > 1 {
            self.error_here("E-SYN-102", "empty row after trailing `;` in list literal");
            return None;
        }
        if !self.eat(&TokenKind::RBracket) {
            self.error_here("E-SYN-102", "expected `]` to close list");
        }
        if rows.len() == 1 {
            return Some(std::mem::take(&mut rows[0]));
        }
        let width = rows[0].len();
        for row in &rows {
            if row.len() != width {
                self.error_here(
                    "E-SYN-102",
                    format!(
                        "`;` rows must have uniform cell counts: expected {width}, found {}",
                        row.len()
                    ),
                );
                return None;
            }
        }
        Some(
            rows.into_iter()
                .map(|row_cells| {
                    let source = row_cells
                        .first()
                        .zip(row_cells.last())
                        .map(|(first, last)| first.source.cover(last.source))
                        .unwrap_or_default();
                    Expr {
                        kind: ExprKind::List(row_cells),
                        source,
                    }
                })
                .collect(),
        )
    }
}
