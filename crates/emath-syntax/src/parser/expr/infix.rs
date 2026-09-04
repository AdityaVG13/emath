//! Binary/infix precedence levels: iff, imply, or, and, unit queries, comparisons, multiplicative, notation infix.

use super::*;

impl super::super::Parser {
    // ---- B12: logic connectives ==> and <==>) -------------------------

    /// `<==>` — biconditional (lowest precedence, left-associative).
    pub(super) fn parse_iff(&mut self, depth: usize) -> Option<Expr> {
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
    pub(super) fn parse_imply(&mut self, depth: usize) -> Option<Expr> {
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

    pub(super) fn parse_or(&mut self, depth: usize) -> Option<Expr> {
        let mut left = self.parse_and(depth)?;
        loop {
            if self.skip_continuation_lines() {
                continue;
            }
            if !matches!(self.peek(), TokenKind::Keyword(Keyword::Or))
                && (!matches!(self.peek(), TokenKind::Pipe) || self.suppress_pipe_or)
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

    pub(super) fn parse_and(&mut self, depth: usize) -> Option<Expr> {
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
    /// Precedence just above `==`; `unit`/`dimension` activate only before `of`.
    pub(super) fn parse_unit_query(&mut self, depth: usize) -> Option<Expr> {
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
        // 04 §6.4: `a ≈ b` /
        // `a ~= b` builds the Approx expr; the optional
        // `within rtol=…, atol=…` clause (either key, either order, at
        // least one) is the DECLARED tolerance and is recorded — never
        // dropped. The tolerance-less form still parses; refusing it is
        // admission's job (E-APPROX-TOL), keeping parse lossless.
        if matches!(self.peek(), TokenKind::TildeEq) {
            return self.parse_approx_tail(left);
        }
        Some(left)
    }

    /// Shared tail after a parsed left operand when the next token is
    /// `≈` / `~=`: parse the right side and the optional declared
    /// tolerance clause. Used from expression position (parse_unit_query)
    /// and from ident-led statement position (`y ≈ ...`).
    pub(in crate::parser) fn parse_approx_tail(&mut self, left: Expr) -> Option<Expr> {
        self.advance(); // `≈` / `~=`
        let right = self.parse_additive(0)?;
        let mut span = left.source.cover(right.source);
        let tolerance = self.parse_approx_tolerance_clause(&mut span)?;
        Some(Expr {
            kind: ExprKind::Approx {
                left: Box::new(left),
                right: Box::new(right),
                tolerance: tolerance.map(Box::new),
            },
            source: span,
        })
    }

    /// `within rtol=…, atol=…` — the declared tolerance clause on an
    /// `≈` edge. Keys may appear in either order and either may be
    /// omitted, but the clause must carry at least one key; anything
    /// else is a typed refusal, never a silent drop.
    pub(super) fn parse_approx_tolerance_clause(
        &mut self,
        span: &mut Span,
    ) -> Option<Option<ApproxTolerance>> {
        if !matches!(self.peek(), TokenKind::Ident(word) if word == "within") {
            return Some(None);
        }
        self.advance();
        let mut rtol = None;
        let mut atol = None;
        loop {
            let TokenKind::Ident(key) = self.peek().clone() else {
                self.error_here(
                    "E-SYN-101",
                    "expected `rtol` or `atol` in the `within` tolerance clause",
                );
                return None;
            };
            if key != "rtol" && key != "atol" {
                self.error_here(
                    "E-SYN-101",
                    "unknown tolerance key (expected `rtol` or `atol`)",
                );
                return None;
            }
            self.advance();
            if !self.eat(&TokenKind::Eq) {
                self.error_here("E-SYN-111", "expected `=` after the tolerance key");
                return None;
            }
            let value = self.parse_additive(0)?;
            *span = span.cover(value.source);
            match key.as_str() {
                "rtol" if rtol.is_none() => rtol = Some(value),
                "atol" if atol.is_none() => atol = Some(value),
                _ => {
                    self.error_here(
                        "E-SYN-101",
                        "duplicate tolerance key in the `within` clause",
                    );
                    return None;
                }
            }
            if self.eat(&TokenKind::Comma) {
                continue;
            }
            break;
        }
        if rtol.is_none() && atol.is_none() {
            self.error_here(
                "E-SYN-101",
                "the `within` clause must declare rtol, atol, or both",
            );
            return None;
        }
        Some(Some(ApproxTolerance { rtol, atol }))
    }

    pub(super) fn parse_comparison(&mut self, depth: usize) -> Option<Expr> {
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

    pub(in crate::parser) fn parse_additive(&mut self, depth: usize) -> Option<Expr> {
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

    pub(super) fn parse_multiplicative(&mut self, depth: usize) -> Option<Expr> {
        let mut left = self.parse_notation_infix(depth, super::super::CUSTOM_OP_MIN_PRECEDENCE)?;
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
            let right = self.parse_notation_infix(depth, super::super::CUSTOM_OP_MIN_PRECEDENCE)?;
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

    /// Custom-operator infix layer (notation declarations). Binds tighter
    /// than `* /` and looser than unary prefix, so `a ⊕ b * c` is
    /// `(a ⊕ b) * c` and `4 * x ⊕ 2` is `4 * (x ⊕ 2)`. Declared
    /// precedences order custom operators against each other (higher
    /// binds tighter); `infixl`/`infix` are left-associative, `infixr`
    /// right-associative via the classic precedence-climbing cut. Glyph
    /// uses desugar to plain calls of the canonical target (N5: the
    /// semantic IR is notation-agnostic).
    pub(super) fn parse_notation_infix(
        &mut self,
        depth: usize,
        min_precedence: u32,
    ) -> Option<Expr> {
        let mut left = self.parse_unary(depth)?;
        loop {
            if self.skip_continuation_lines() {
                continue;
            }
            let (target, precedence, left_assoc) = match self.peek().clone() {
                TokenKind::Ident(name) => match self.notations.get(&name) {
                    Some(op)
                        if matches!(
                            op.fixity,
                            NotationFixity::Infix
                                | NotationFixity::InfixLeft
                                | NotationFixity::InfixRight
                        ) && op.precedence >= min_precedence =>
                    {
                        (
                            op.target.clone(),
                            op.precedence,
                            op.fixity != NotationFixity::InfixRight,
                        )
                    }
                    _ => break,
                },
                _ => break,
            };
            self.advance();
            if self.skip_continuation_lines() {
                // operator-first continuation
            }
            let right = self.parse_notation_infix(
                depth,
                if left_assoc {
                    precedence + 1
                } else {
                    precedence
                },
            )?;
            let span = left.source.cover(right.source);
            left = self.notation_call(&target, vec![left, right], span);
        }
        Some(left)
    }
}
