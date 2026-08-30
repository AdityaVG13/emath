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
    /// Precedence just above `==`; `unit`/`dimension` activate only before `of`.
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
        // 04 §6.4 (bead emath-r3-approx-operator-depc): `a ≈ b` /
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
    pub(super) fn parse_approx_tail(&mut self, left: Expr) -> Option<Expr> {
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
    fn parse_approx_tolerance_clause(
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
        let mut left = self.parse_notation_infix(depth, super::CUSTOM_OP_MIN_PRECEDENCE)?;
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
            let right = self.parse_notation_infix(depth, super::CUSTOM_OP_MIN_PRECEDENCE)?;
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
    fn parse_notation_infix(&mut self, depth: usize, min_precedence: u32) -> Option<Expr> {
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

    fn parse_unary(&mut self, depth: usize) -> Option<Expr> {
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

    /// Table literal (U9): `|x y| 1, 2 | 3, 4 |`. Reached only when a pipe
    /// starts a primary (arm-leading and infix pipes never land here).
    /// Requires ≥2 header idents so `| cond => …` and `|x|`-shaped pipes
    /// fall through to the cases/or grammar unchanged.
    fn parse_table_literal(&mut self) -> Option<Expr> {
        let start = self.current_span();
        self.advance(); // opening `|`
        let mut headers = Vec::new();
        while let TokenKind::Ident(name) = self.peek().clone() {
            headers.push(name);
            self.advance();
        }
        if headers.len() < 2 {
            self.error_here(
                "E-SYN-102",
                "table literal needs at least two `|`-delimited columns; single-column \
                 `|…|` is ambiguous with cases arms and infix `or`",
            );
            return None;
        }
        if !self.eat(&TokenKind::Pipe) {
            self.error_here("E-SYN-102", "expected `|` to close table headers");
            return None;
        }
        // Inside cells, `|` is a row delimiter, never infix `or` — same
        // suppression the cases-arm parser uses. Saved/restored so a table
        // nested in a cases arm keeps the outer suppression state.
        let outer_suppress = self.suppress_pipe_or;
        self.suppress_pipe_or = true;
        let table = self.parse_table_body(start, headers);
        self.suppress_pipe_or = outer_suppress;
        table
    }

    fn parse_table_body(&mut self, start: emath_core::Span, headers: Vec<String>) -> Option<Expr> {
        let mut rows: Vec<Vec<Expr>> = Vec::new();
        loop {
            // Cells are comma-separated; a row ends at the next `|`.
            let mut cells: Vec<Expr> = Vec::new();
            loop {
                if matches!(self.peek(), TokenKind::Pipe | TokenKind::Eof) {
                    break;
                }
                if self.eat(&TokenKind::Comma) {
                    continue;
                }
                cells.push(self.parse_expr()?);
            }
            if cells.is_empty() {
                self.error_here("E-SYN-102", "table rows must have at least one cell");
                return None;
            }
            rows.push(cells);
            if !self.eat(&TokenKind::Pipe) {
                self.error_here("E-SYN-102", "expected `|` to close each table row");
                return None;
            }
            // After the row-closing `|`: another row starts with an
            // expression token; anything else closes the table. A further
            // `|` closes too — it belongs to the enclosing cases arm, not
            // to this table (U1 ambiguity scan).
            let closes = matches!(
                self.peek(),
                TokenKind::Eof
                    | TokenKind::Newline
                    | TokenKind::Dedent
                    | TokenKind::RBracket
                    | TokenKind::RParen
                    | TokenKind::RBrace
                    | TokenKind::Comma
                    | TokenKind::Pipe
                    | TokenKind::Keyword(_)
            );
            if closes {
                break;
            }
        }
        let width = headers.len();
        for row in &rows {
            if row.len() != width {
                self.error_here(
                    "E-SYN-102",
                    format!(
                        "table rows must have {width} cells (one per column), found {}",
                        row.len()
                    ),
                );
                return None;
            }
        }
        Some(Expr {
            kind: ExprKind::Table { headers, rows },
            source: start.cover(self.last_span()),
        })
    }

    /// Nabla-family call parse (pack o6jp). Targets and shapes mirror the
    /// EXISTING builtins exactly (checked against `admit/lowering.rs`):
    /// - `Grad`:  `∇(u, dx)`       → `core::pde::gradient(u, dx)` (2 args)
    /// - `Lap`:   `∇²(u, dx)`      → `core::pde::laplacian_2d(u, dx)`
    ///   (u is a Matrix field; ONE cell width — 2D is implicit in the field)
    /// - `Div`:   `∇·(vx, vy, dx)` → `core::pde::div_2d(vx, vy, dx)`
    ///   (two Matrix fields + one cell width)
    /// - `Curl`:  `∇×(u, v, dx)`   → 2D scalar curl sugar
    ///   `gradient_2d_x(v, dx) − gradient_2d_y(u, dx)` (u, v Matrix
    ///   fields, one shared cell width). 3D arity (4 args) refuses typed —
    ///   the 3D curl OperatorDef is pending in the stencil world.
    /// Spacing stays explicit at every call site: mesh spacing is world
    /// data, never ambient (operator-arity honesty).
    fn parse_nabla_call(&mut self, form: NablaForm) -> Option<Expr> {
        let start = self.current_span();
        self.advance(); // glyph
        if !self.eat(&TokenKind::LParen) {
            self.error_here("E-SYN-102", "expected `(` after a nabla operator");
            return None;
        }
        let mut args: Vec<Expr> = Vec::new();
        while !matches!(self.peek(), TokenKind::RParen | TokenKind::Eof) {
            if self.eat(&TokenKind::Comma) {
                continue;
            }
            args.push(self.parse_expr()?);
        }
        if !self.eat(&TokenKind::RParen) {
            self.error_here("E-SYN-102", "expected `)` to close nabla arguments");
            return None;
        }
        let target = |segments: &[&str]| Expr {
            kind: ExprKind::Path {
                segments: segments.iter().map(|s| (*s).to_string()).collect(),
                generics: None,
            },
            source: start,
        };
        let make_call = |target: Expr, args: Vec<Expr>, source: emath_core::Span| Expr {
            kind: ExprKind::Call {
                function: Box::new(target),
                args,
            },
            source,
        };
        let span = start.cover(self.last_span());
        match form {
            NablaForm::Grad => {
                if args.len() != 2 {
                    self.error_here(
                        "E-SYN-102",
                        format!(
                            "`∇` (gradient) takes (u, dx), found {} arguments",
                            args.len()
                        ),
                    );
                    return None;
                }
                Some(make_call(target(&["core", "pde", "gradient"]), args, span))
            }
            NablaForm::Lap => {
                if args.len() != 2 {
                    self.error_here(
                        "E-SYN-102",
                        format!(
                            "`∇²` (stencil Laplacian) takes (u, dx) — a Matrix field and its cell width; found {} arguments",
                            args.len()
                        ),
                    );
                    return None;
                }
                Some(make_call(
                    target(&["core", "pde", "laplacian_2d"]),
                    args,
                    span,
                ))
            }
            NablaForm::Div => {
                if args.len() != 3 {
                    self.error_here(
                        "E-SYN-102",
                        format!(
                            "`∇·` (divergence) takes (vx, vy, dx), found {} arguments",
                            args.len()
                        ),
                    );
                    return None;
                }
                Some(make_call(target(&["core", "pde", "div_2d"]), args, span))
            }
            NablaForm::Curl => {
                if args.len() == 4 {
                    self.error_here(
                        "E-SYN-101",
                        "3D curl has no discrete OperatorDef in the stencil world yet; \
                         the 2D scalar form `∇×(u, v, dx)` is the admitted spelling",
                    );
                    return None;
                }
                if args.len() != 3 {
                    self.error_here(
                        "E-SYN-102",
                        format!("`∇×` takes (u, v, dx), found {} arguments", args.len()),
                    );
                    return None;
                }
                let [u, v, dx] = args.try_into().ok()?;
                // ∂v/∂x − ∂u/∂y, through existing component gradients
                // (one shared cell width).
                let dv_dx = make_call(
                    target(&["core", "pde", "gradient_2d_x"]),
                    vec![v, dx.clone()],
                    span,
                );
                let du_dy = make_call(target(&["core", "pde", "gradient_2d_y"]), vec![u, dx], span);
                Some(Expr {
                    kind: ExprKind::Binary {
                        op: BinaryOp::Sub,
                        left: Box::new(dv_dx),
                        right: Box::new(du_dy),
                    },
                    source: span,
                })
            }
        }
    }

    /// Optional distribution tag on a measurement literal (`~ normal`,
    /// `~ uniform`, `~ lognormal`; spec 04 section 1.5). The name is
    /// recorded raw; vocabulary validation is admission's job.
    fn parse_distribution_tag(&mut self) -> Result<Option<String>, ()> {
        if !matches!(self.peek(), TokenKind::Tilde) {
            return Ok(None);
        }
        self.advance();
        let TokenKind::Ident(name) = self.peek().clone() else {
            self.error_here(
                "E-SYN-110",
                "expected a distribution name after `~` (normal | uniform | lognormal)",
            );
            return Err(());
        };
        self.advance();
        Ok(Some(name))
    }

    fn parse_primary(&mut self, depth: usize) -> Option<Expr> {
        let start = self.current_span();
        match self.peek().clone() {
            TokenKind::Int(text) => {
                self.advance();
                // `rational_literal = integer "//" integer` (surface.ebnf).
                // `//` lexes as SlashSlash; fold here so `3//7` is one
                // primary, not Int("3") plus a leftover `//`.
                if matches!(self.peek(), TokenKind::SlashSlash) {
                    self.advance();
                    match self.peek().clone() {
                        TokenKind::Int(denom) => {
                            self.advance();
                            Some(Expr {
                                kind: ExprKind::Rational { numer: text, denom },
                                source: start.cover(self.last_span()),
                            })
                        }
                        other => {
                            self.error_here(
                                "E-SYN-101",
                                format!(
                                    "exact rational `//` requires an integer denominator, found {}",
                                    other.describe()
                                ),
                            );
                            None
                        }
                    }
                } else {
                    Some(Expr {
                        kind: ExprKind::Int(text),
                        source: start,
                    })
                }
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
                // Measurement literal, explicit form (spec 04 section 1.5):
                // `1.50 ± 0.02 [~ dist]` folds into one Measured literal.
                // X6: `±` in core IS the measurement literal.
                if matches!(self.peek(), TokenKind::PlusMinus) {
                    self.advance();
                    let TokenKind::Float(uncertainty) = self.peek().clone() else {
                        self.error_here(
                            "E-SYN-110",
                            "expected a number after `±` in a measurement literal",
                        );
                        return None;
                    };
                    self.advance();
                    // `Err(())` follows an already-emitted E-SYN-110; `.ok()?`
                    // aborts the Option-returning primary without dropping it.
                    let distribution = self.parse_distribution_tag().ok()?;
                    return Some(Expr {
                        kind: ExprKind::Measured {
                            value: text,
                            uncertainty,
                            uncertainty_digits: String::new(),
                            distribution,
                        },
                        source: start.cover(self.last_span()),
                    });
                }
                Some(Expr {
                    kind: ExprKind::Float(text),
                    source: start,
                })
            }
            // Measurement literal, attached parenthetical form (CODATA):
            // `0.5012(3)` / `6.67430(15)e-11` lexed as one token in slice 1.
            TokenKind::FloatUncertainty { number, digits } => {
                self.advance();
                let distribution = self.parse_distribution_tag().ok()?;
                Some(Expr {
                    kind: ExprKind::Measured {
                        value: number,
                        uncertainty: String::new(),
                        uncertainty_digits: digits,
                        distribution,
                    },
                    source: start.cover(self.last_span()),
                })
            }
            TokenKind::Str(value) => {
                self.advance();
                // U8 (emath-r3-string-interp-og2e): the interpolation
                // grammar is validated at parse — purity (holes carry
                // only names/paths), the fixed format spec, and brace
                // escapes. The template value stays raw in the `Str`
                // literal; runtime substitution belongs to the string
                // world, which is outside the Phase 1 subset (all
                // string values refuse at admission today).
                self.validate_interpolation(&value, start)?;
                Some(Expr {
                    kind: ExprKind::Str(value),
                    source: start,
                })
            }
            TokenKind::Question => {
                self.advance();
                Some(Expr {
                    kind: ExprKind::Path {
                        segments: vec!["Hole".to_string()],
                        generics: None,
                    },
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
            TokenKind::Pipe => {
                // fdby: `|0⟩` — a pipe followed by an integer label and
                // `⟩` is a braket ket (checked BEFORE the table
                // reading, whose headers are identifiers). Glyphs are
                // opt-in: unmounted, the refusal names the pack.
                if matches!(self.peek_at(1), TokenKind::Int(_))
                    && matches!(self.peek_at(2), TokenKind::RAngle)
                {
                    if !self.mounted_packs.contains("braket") {
                        self.error_here(
                            "E-SYN-101",
                            "braket glyph outside the notation pack; mount it first with \
                             `use sci::physics::notation::braket` (glyphs are opt-in, never ambient)",
                        );
                        return None;
                    }
                    self.advance(); // `|`
                    let TokenKind::Int(label) = self.peek().clone() else {
                        unreachable!("shape checked above");
                    };
                    self.advance();
                    if !self.eat(&TokenKind::RAngle) {
                        self.error_here("E-SYN-101", "expected `⟩` to close the ket");
                        return None;
                    }
                    // fdby: `|i⟩⟨j|` — the juxtaposed bra is the outer
                    // product (projector), a constant matrix on the
                    // real 2-level carrier.
                    if matches!(self.peek(), TokenKind::LAngle)
                        && matches!(self.peek_at(1), TokenKind::Int(_))
                        && matches!(self.peek_at(2), TokenKind::Pipe)
                    {
                        self.advance(); // `⟨`
                        let TokenKind::Int(col) = self.peek().clone() else {
                            unreachable!("shape checked above");
                        };
                        self.advance();
                        if !self.eat(&TokenKind::Pipe) {
                            self.error_here(
                                "E-SYN-101",
                                "expected `|` to close the bra in the projector",
                            );
                            return None;
                        }
                        return self.braket_projector(start, &label, &col);
                    }
                    return self.braket_operand_expr(&BraketOperand::Label(label), start);
                }
                // U9: a pipe that STARTS a primary can only be a table
                // literal (`|x y| 1, 2 | 3, 4 |`); cases arms and infix
                // `or` consume their pipes before reaching this position.
                self.parse_table_literal()
            }
            TokenKind::LAngle => {
                // fdby: bra-led braket forms (`⟨φ|ψ⟩`, `⟨φ|P|ψ⟩`, the
                // standalone bra `⟨φ|`). Opt-in like the ket side.
                if !self.mounted_packs.contains("braket") {
                    self.error_here(
                        "E-SYN-101",
                        "braket glyph outside the notation pack; mount it first with \
                         `use sci::physics::notation::braket` (glyphs are opt-in, never ambient)",
                    );
                    return None;
                }
                self.advance(); // `⟨`
                self.parse_bra_form(start, depth)
            }
            TokenKind::Nabla(form) => {
                // Nabla pack (o6jp): glyphs desugar to EXISTING
                // core::pde builtins only when the pack is mounted
                // (`use sci::physics::notation::nabla`); unmounted, the
                // refusal names the import — never a silent ident.
                if !self.mounted_packs.contains("nabla") {
                    self.error_here(
                        "E-SYN-101",
                        "nabla glyph outside the notation pack; mount it first with \
                         `use sci::physics::notation::nabla` (glyphs are opt-in, never ambient)",
                    );
                    return None;
                }
                self.parse_nabla_call(form)
            }
            TokenKind::LBrace => {
                // B01+U3: `{a, b, c}` set literal, `{n in d if g}`
                // comprehension, `{}` empty set. Bare `{name: value}`
                // (record spelling without a path prefix) is ambiguous
                // between records and sets — refuse E-SYN-154 (X12: one
                // ELP scan covers both `{}` forms).
                self.advance(); // `{`
                if self.eat(&TokenKind::RBrace) {
                    return Some(Expr {
                        kind: ExprKind::Set(Vec::new()),
                        source: start.cover(self.last_span()),
                    });
                }
                let first = {
                    // Suppress postfix-if so the comprehension guard stays
                    // at brace level (binders do the same in parse_binders);
                    // `if` here is the guard, not a conditioned value.
                    let prev_flag = self.suppress_postfix_if;
                    self.suppress_postfix_if = true;
                    let parsed = self.parse_expr_depth(depth + 1);
                    self.suppress_postfix_if = prev_flag;
                    parsed?
                };
                match self.peek() {
                    // `{name: ...}` without a path prefix: ambiguous, refuse.
                    TokenKind::Colon => {
                        self.error_here(
                            "E-SYN-154",
                            "ambiguous brace: `{name: value}` without a path prefix is not an \
                             inline record; prefix it (`Point:{...}`) or write a set literal",
                        );
                        return None;
                    }
                    // Set literal: the first element parsed without `in`.
                    TokenKind::Comma | TokenKind::RBrace => {
                        let mut items = vec![first];
                        while self.eat(&TokenKind::Comma) {
                            if matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
                                break;
                            }
                            items.push(self.parse_expr_depth(depth + 1)?);
                        }
                        if !self.eat(&TokenKind::RBrace) {
                            self.error_here("E-SYN-102", "expected `}` to close set literal");
                        }
                        Some(Expr {
                            kind: ExprKind::Set(items),
                            source: start.cover(self.last_span()),
                        })
                    }
                    // Comprehension: `{element in domain if guard}` — the
                    // element parse consumed `in` as a membership binary
                    // (X12 single ambiguity scan); brace position re-reads
                    // it as the comprehension binding. A non-membership
                    // first element followed by `if`/`}` is a parse error.
                    _ => {
                        let ExprKind::Binary {
                            op: BinaryOp::In,
                            left,
                            right,
                        } = &first.kind
                        else {
                            self.error_here(
                                "E-SYN-102",
                                "expected `,` (set element), `in` (comprehension), or `}` to \
                                 close a brace expression",
                            );
                            return None;
                        };
                        let element = left.clone();
                        let domain = right.clone();
                        let guard = self.parse_binder_guard();
                        if !self.eat(&TokenKind::RBrace) {
                            self.error_here("E-SYN-102", "expected `}` to close comprehension");
                        }
                        let ExprKind::Path { segments, .. } = &element.kind else {
                            self.error_here(
                                "E-SYN-101",
                                "comprehension element must be the bound name in Phase 1",
                            );
                            return None;
                        };
                        let Some(var) = segments.last().cloned() else {
                            self.error_here("E-SYN-101", "comprehension element must be a name");
                            return None;
                        };
                        Some(Expr {
                            kind: ExprKind::SetComprehension {
                                element,
                                var,
                                domain,
                                guard,
                            },
                            source: start.cover(self.last_span()),
                        })
                    }
                }
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
            TokenKind::Keyword(Keyword::Jacobian) => {
                self.advance();
                // B06 (emath-9bj1, Track A3): `jacobian(<expr>) wrt
                // v1, v2` is parse-time sugar for the uniform AST/IR
                // Jacobian value form — a matrix literal whose cells
                // are the existing dual-number forward-mode
                // `derivative(...) wrt v` nodes, the same form a user
                // writes by hand (one column per `wrt` variable). A
                // list body `[f1, f2]` splits into one row per
                // component; a scalar body is a single row
                // (`Matrix[1, n]`). `wrt` is mandatory so a malformed
                // jacobian never silently half-admits.
                let value = self.parse_postfix(depth + 1)?;
                if !self.eat(&TokenKind::Keyword(Keyword::Wrt)) {
                    self.error_here("E-SYN-111", "expected `wrt` after `jacobian` body");
                    return None;
                }
                let mut vars = vec![self.parse_expr_depth(depth + 1)?];
                while self.eat(&TokenKind::Comma) {
                    vars.push(self.parse_expr_depth(depth + 1)?);
                }
                let components: Vec<Expr> = match &value.kind {
                    ExprKind::List(items) => items.clone(),
                    _ => vec![value],
                };
                let rows: Vec<Expr> = components
                    .into_iter()
                    .map(|component| {
                        let cells: Vec<Expr> = vars
                            .iter()
                            .map(|var| Expr {
                                kind: ExprKind::Derivative {
                                    value: Box::new(component.clone()),
                                    wrt: Some(vec![var.clone()]),
                                    kind: DerivativeKind::Plain,
                                    holding: Vec::new(),
                                },
                                source: start.cover(self.last_span()),
                            })
                            .collect();
                        Expr {
                            kind: ExprKind::List(cells),
                            source: start.cover(self.last_span()),
                        }
                    })
                    .collect();
                Some(Expr {
                    kind: ExprKind::List(rows),
                    source: start.cover(self.last_span()),
                })
            }
            TokenKind::Keyword(Keyword::Match) => {
                // U6 (emath-r3-match-expr-dnbd): `match subject { pattern
                // => value, ... }` is expression-position sugar for
                // `cases` (U1). Literal patterns become `subject ==
                // pattern` conditions; the mandatory FINAL catch-all
                // (`_`, or a binding name) becomes the else arm, so
                // totality is a parse-time guarantee. The subject is a
                // postfix expression: compound subjects need parentheses
                // (`match (a + b) { ... }`), keeping the opening brace
                // unambiguous with record spelling (`Path:{...}`).
                self.advance(); // `match`
                let subject = self.parse_postfix(depth + 1)?;
                if !self.eat(&TokenKind::LBrace) {
                    self.error_here("E-SYN-101", "expected `{` to open match arms");
                    return None;
                }
                self.parse_match_body(start, Box::new(subject), depth)
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
                            start, var, false, // is_sample = false
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
                            start, var, true, // is_sample = true
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
                            self.error_here(
                                "E-SYN-111",
                                "expected `:` after series binder variables",
                            );
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
                    // B23 (emath-r3-graphs-ybob): `graph { nodes ; edges }`
                    // — contextual only before a brace; `graph` elsewhere
                    // is a regular identifier. Desugars to the plain List
                    // shape the graph world consumes (no tree variant):
                    // [nodes…, edges…] with edges
                    // [from, to, weight, directed].
                    if name == "graph" && matches!(self.peek_at(1), TokenKind::LBrace) {
                        self.advance(); // `graph`
                        self.advance(); // `{`
                        return Some(self.parse_graph_literal(start, depth)?);
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
                    // Keyword segments (`core::logic::not`) are accepted
                    // after a path separator, mirroring the notation
                    // target-path rule, so a qualified call spells
                    // exactly like the notation desugar.
                    match self.peek().clone() {
                        TokenKind::Ident(segment) => {
                            segments.push(segment);
                            self.advance();
                        }
                        TokenKind::Keyword(keyword) => {
                            segments.push(keyword.spelling().to_string());
                            self.advance();
                        }
                        _ => break,
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
                // U3: `Point:{x: 1.0, y: 2.0}` — path-prefixed braces are an
                // inline record literal. The path prefix is what makes the
                // brace unambiguous under the X12 one-ELP scan; the bare
                // form refuses E-SYN-154 in the LBrace arm above.
                if matches!(self.peek(), TokenKind::Colon)
                    && matches!(self.peek_at(1), TokenKind::LBrace)
                    && generics.is_none()
                {
                    self.advance(); // `:`
                    self.advance(); // `{`
                    let mut fields = Vec::new();
                    while !matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
                        let TokenKind::Ident(field) = self.peek().clone() else {
                            self.error_here(
                                "E-SYN-101",
                                "expected a field name in inline record literal",
                            );
                            return None;
                        };
                        self.advance();
                        if !self.eat(&TokenKind::Colon) {
                            self.error_here("E-SYN-111", "expected `:` after record field name");
                            return None;
                        }
                        let value = self.parse_expr_depth(depth + 1)?;
                        fields.push((field, value));
                        if !self.eat(&TokenKind::Comma) {
                            break;
                        }
                    }
                    if !self.eat(&TokenKind::RBrace) {
                        self.error_here("E-SYN-102", "expected `}` to close inline record");
                    }
                    return Some(Expr {
                        kind: ExprKind::Record {
                            type_path: segments,
                            fields,
                        },
                        source: start.cover(self.last_span()),
                    });
                }
                Some(Expr {
                    kind: ExprKind::Path { segments, generics },
                    source: start.cover(self.last_span()),
                })
            }
            other => {
                // F2 (emath-r3-layout-ynde): a NEWLINE after a binary
                // operator is a hanging infix, not a statement boundary;
                // teach the bracket idiom instead of a bare type error.
                if matches!(other, TokenKind::Newline) {
                    let previous = self
                        .tokens
                        .get(self.pos.checked_sub(1).unwrap_or(0))
                        .map(|token| &token.kind);
                    if let crate::layout::LayoutExplanation::HangingInfix =
                        crate::layout::classify_line_break(previous)
                    {
                        self.error_here(
                            crate::layout::E_SYN_HANGING_INFIX,
                            format!(
                                "expected the right-hand side of an infix expression, found end of line. {}",
                                crate::layout::LayoutExplanation::HangingInfix.help()
                            ),
                        );
                        return None;
                    }
                }
                self.error_here(
                    "E-SYN-110",
                    format!("expected an expression, found {}", other.describe()),
                );
                None
            }
        }
    }

    /// B04: Parse the body of a `limit x -> T[+|-]: body` expression; the
    /// target parses at multiplicative level so `+`/`-` before `:` is a
    /// direction suffix (complex targets need parens).
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

    /// U1: Parse a `cases [subject]: | c1 => e1 | else => eN` body; arms use
    /// `|`/`=>` and a mandatory `else` arm enforces totality.
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
        let else_arm;
        // `|` is an arm delimiter here, not `or`.
        self.suppress_pipe_or = true;
        loop {
            self.skip_newlines();
            if !self.eat(&TokenKind::Pipe) {
                self.suppress_pipe_or = false;
                if arms.is_empty() {
                    self.error_here("E-SYN-110", "expected `|` to start a cases arm");
                } else {
                    // Totality is `| else => ...`. After a condition arm a
                    // missing `|` is a missing else, not another arm.
                    self.error_here(
                        "E-SYN-110",
                        "cases expression requires a mandatory `else` arm",
                    );
                }
                return None;
            }
            self.skip_newlines();
            // Check for `else` arm.
            if matches!(self.peek(), TokenKind::Keyword(Keyword::Else)) {
                self.advance(); // `else`
                if !self.eat(&TokenKind::Arrow) {
                    self.suppress_pipe_or = false;
                    self.error_here("E-SYN-101", "expected `=>` after `else` in cases arm");
                    return None;
                }
                self.skip_newlines();
                let Some(value) = self.parse_expr_depth(depth + 1) else {
                    self.suppress_pipe_or = false;
                    return None;
                };
                else_arm = Some(Box::new(value));
                break;
            }
            // Regular arm: `| condition => value`
            let Some(condition) = self.parse_expr_depth(depth + 1) else {
                self.suppress_pipe_or = false;
                return None;
            };
            if !self.eat(&TokenKind::Arrow) {
                self.suppress_pipe_or = false;
                self.error_here("E-SYN-101", "expected `=>` in cases arm");
                return None;
            }
            self.skip_newlines();
            let Some(value) = self.parse_expr_depth(depth + 1) else {
                self.suppress_pipe_or = false;
                return None;
            };
            arms.push((condition, value));
        }
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

    /// U6: Parse the brace body of a match expression and desugar to
    /// `ExprKind::Cases`. Literal patterns become `subject == pattern`
    /// conditions; the mandatory final catch-all arm (`_` or a binding
    /// name) becomes the else arm. A binding pattern substitutes the
    /// subject for the bound name in its value; binder, comprehension,
    /// and limit variables of the same name shadow it (lexical scoping).
    fn parse_match_body(&mut self, start: Span, subject: Box<Expr>, depth: usize) -> Option<Expr> {
        let mut arms: Vec<(Expr, Expr)> = Vec::new();
        let mut else_arm: Option<Box<Expr>> = None;
        loop {
            self.skip_newlines();
            if self.eat(&TokenKind::RBrace) {
                break;
            }
            let pattern = self.parse_match_pattern(depth)?;
            if !self.eat(&TokenKind::Arrow) {
                self.error_here("E-SYN-101", "expected `=>` in match arm");
                return None;
            }
            let Some(value) = self.parse_expr_depth(depth + 1) else {
                return None;
            };
            match pattern {
                MatchPattern::Literal(literal) => {
                    let condition = Expr {
                        kind: ExprKind::Binary {
                            op: BinaryOp::Eq,
                            left: Box::new((*subject).clone()),
                            right: Box::new(literal),
                        },
                        source: start.cover(self.last_span()),
                    };
                    arms.push((condition, value));
                    self.skip_newlines();
                    if !self.eat(&TokenKind::Comma) {
                        if self.eat(&TokenKind::RBrace) {
                            break;
                        }
                        self.error_here("E-SYN-101", "expected `,` between match arms");
                        return None;
                    }
                }
                MatchPattern::CatchAll(binding) => {
                    // Totality arm: it must close the match. A catch-all
                    // before the end would make every later arm
                    // unreachable under first-match-wins.
                    self.skip_newlines();
                    if !self.eat(&TokenKind::RBrace) {
                        self.error_here(
                            "E-SYN-101",
                            "the catch-all match arm must be the last arm (first-match-wins makes later arms unreachable)",
                        );
                        return None;
                    }
                    let value = match binding {
                        Some(name) => substitute_bound(value, &subject, &name),
                        None => value,
                    };
                    else_arm = Some(Box::new(value));
                    break;
                }
            }
        }
        let Some(else_arm) = else_arm else {
            self.error_here(
                "E-SYN-110",
                "match expression requires a final catch-all arm (`_ => ...` or `name => ...`); totality is mandatory",
            );
            return None;
        };
        if arms.is_empty() {
            self.error_here(
                "E-SYN-110",
                "match expression requires at least one pattern arm before the catch-all",
            );
            return None;
        }
        Some(Expr {
            kind: ExprKind::Cases {
                subject: Some(subject),
                arms,
                else_arm,
            },
            source: start.cover(self.last_span()),
        })
    }

    /// One U6 match pattern: a literal (Int/Float/Str/Bool, with an
    /// optional leading `-` for numeric literals), the `_` wildcard, or
    /// a binding name. Patterns are deliberately not full expressions:
    /// in value position a bare name means "bind the subject", so a
    /// binding pattern can never be confused with a value pattern.
    fn parse_match_pattern(&mut self, _depth: usize) -> Option<MatchPattern> {
        let start = self.current_span();
        match self.peek().clone() {
            TokenKind::Ident(name) if name == "_" => {
                self.advance();
                Some(MatchPattern::CatchAll(None))
            }
            TokenKind::Int(text) => {
                self.advance();
                Some(MatchPattern::Literal(Expr {
                    kind: ExprKind::Int(text),
                    source: start,
                }))
            }
            TokenKind::Float(text) => {
                self.advance();
                Some(MatchPattern::Literal(Expr {
                    kind: ExprKind::Float(text),
                    source: start,
                }))
            }
            TokenKind::Str(text) => {
                self.advance();
                Some(MatchPattern::Literal(Expr {
                    kind: ExprKind::Str(text),
                    source: start,
                }))
            }
            TokenKind::Keyword(Keyword::True) => {
                self.advance();
                Some(MatchPattern::Literal(Expr {
                    kind: ExprKind::Bool(true),
                    source: start,
                }))
            }
            TokenKind::Keyword(Keyword::False) => {
                self.advance();
                Some(MatchPattern::Literal(Expr {
                    kind: ExprKind::Bool(false),
                    source: start,
                }))
            }
            TokenKind::Minus => {
                self.advance();
                let literal = match self.peek().clone() {
                    TokenKind::Int(text) => {
                        self.advance();
                        Expr {
                            kind: ExprKind::Int(text),
                            source: self.last_span(),
                        }
                    }
                    TokenKind::Float(text) => {
                        self.advance();
                        Expr {
                            kind: ExprKind::Float(text),
                            source: self.last_span(),
                        }
                    }
                    other => {
                        self.error_here(
                            "E-SYN-101",
                            format!(
                                "expected a number after `-` in match pattern, found {}",
                                other.describe()
                            ),
                        );
                        return None;
                    }
                };
                Some(MatchPattern::Literal(Expr {
                    kind: ExprKind::Unary {
                        op: UnaryOp::Neg,
                        value: Box::new(literal),
                    },
                    source: start.cover(self.last_span()),
                }))
            }
            TokenKind::Ident(name) => {
                self.advance();
                Some(MatchPattern::CatchAll(Some(name)))
            }
            other => {
                self.error_here(
                    "E-SYN-101",
                    format!("unsupported match pattern, found {}", other.describe()),
                );
                None
            }
        }
    }

    /// B23: parse a graph literal body after `graph {`: nodes
    /// (comma-separated postfix expressions) then an optional `;` and
    /// comma-separated edges. Each edge desugars to
    /// `[from, to, weight, directed]` (weight 1.0 when unspecified;
    /// directed 1.0 for `-->`/`-[w]->`, 0.0 for `-`/`-[w]-`). Edge
    /// syntax exists ONLY in this section, so outside-brace `x--y`
    /// arithmetic is untouched.
    fn parse_graph_literal(&mut self, start: Span, depth: usize) -> Option<Expr> {
        let mut nodes: Vec<Expr> = Vec::new();
        let mut edges: Vec<Expr> = Vec::new();
        let mut in_edges = false;
        loop {
            self.skip_newlines();
            if self.eat(&TokenKind::RBrace) {
                break;
            }
            if self.eat(&TokenKind::Semicolon) {
                if in_edges {
                    self.error_here("E-SYN-101", "duplicate `;` in graph literal");
                    return None;
                }
                in_edges = true;
                continue;
            }
            let Some(first) = self.parse_postfix(depth + 1) else {
                return None;
            };
            if in_edges {
                edges.push(self.parse_graph_edge(first, start, depth)?);
            } else {
                nodes.push(first);
            }
            self.skip_newlines();
            if self.eat(&TokenKind::Comma) {
                continue;
            }
            if matches!(self.peek(), TokenKind::Semicolon) && !in_edges {
                continue;
            }
            if !self.eat(&TokenKind::RBrace) {
                self.error_here("E-SYN-101", "expected `,`, `;`, or `}` in graph literal");
                return None;
            }
            break;
        }
        // Tuple carrier: `[nodes, edges]` as a List would be
        // matrix-interpreted by lowering (rows must be numeric), so the
        // desugar uses Tuple (not matrix-eligible). Admission refuses the
        // tuple via the Phase-1 catch-all until a graph value carrier
        // lands in emath-ir (sets-tub8 Phase B boundary pattern); parse
        // and shape are fully validated here.
        let nodes_list = Expr {
            kind: ExprKind::List(nodes),
            source: start,
        };
        let edges_list = Expr {
            kind: ExprKind::List(edges),
            source: start,
        };
        Some(Expr {
            kind: ExprKind::Tuple(vec![nodes_list, edges_list]),
            source: start.cover(self.last_span()),
        })
    }

    /// One B23 edge: `from` (already parsed) followed by `-->`,
    /// `-[w]->`, `-`, or `-[w]-`, then the target operand. Desugars to
    /// `[from, to, weight, directed]`.
    fn parse_graph_edge(&mut self, from: Expr, start: Span, depth: usize) -> Option<Expr> {
        let directed;
        let weight;
        if self.eat(&TokenKind::EdgeArrow) {
            directed = true;
            weight = Expr {
                kind: ExprKind::Float("1.0".into()),
                source: start,
            };
        } else if self.eat(&TokenKind::Minus) {
            if self.eat(&TokenKind::LBracket) {
                if matches!(self.peek(), TokenKind::RBracket) {
                    self.error_here("E-SYN-101", "empty edge weight in `-[ ]->`");
                    return None;
                }
                weight = self.parse_expr_depth(depth + 1)?;
                if !self.eat(&TokenKind::RBracket) {
                    self.error_here("E-SYN-101", "expected `]` to close the edge weight");
                    return None;
                }
                if self.eat(&TokenKind::Arrow) {
                    directed = true;
                } else if self.eat(&TokenKind::Minus) {
                    directed = false;
                } else {
                    self.error_here("E-SYN-101", "expected `->` or `-` after the edge weight");
                    return None;
                }
            } else {
                directed = false;
                weight = Expr {
                    kind: ExprKind::Float("1.0".into()),
                    source: start,
                };
            }
        } else {
            self.error_here(
                "E-SYN-101",
                "expected an edge operator (`-->`, `-[w]->`, `-`, or `-[w]-`); \
                 `->` is not an edge spelling",
            );
            return None;
        }
        let Some(to) = self.parse_postfix(depth + 1) else {
            self.error_here(
                "E-SYN-101",
                "expected the target operand after the edge operator",
            );
            return None;
        };
        let flag = Expr {
            kind: ExprKind::Float(if directed { "1.0" } else { "0.0" }.into()),
            source: start,
        };
        Some(Expr {
            kind: ExprKind::List(vec![from, to, weight, flag]),
            source: start.cover(self.last_span()),
        })
    }

    /// fdby: parse a bra-led braket form after `⟨` is consumed:
    /// `⟨φ|ψ⟩` (inner product), `⟨φ|P|ψ⟩` (sandwich), or the standalone
    /// bra `⟨φ|` (the conjugated vector; the conjugate is the identity
    /// on the pack's real carrier, so the bra desugars to its operand).
    fn parse_bra_form(&mut self, start: Span, depth: usize) -> Option<Expr> {
        let bra = self.parse_braket_operand(start)?;
        if !self.eat(&TokenKind::Pipe) {
            self.error_here("E-SYN-101", "expected `|` to close the bra");
            return None;
        }
        match self.peek().clone() {
            // `⟨φ|ψ⟩` or `⟨φ|P|ψ⟩`: an operand follows the pipe.
            TokenKind::Int(_) | TokenKind::Ident(_) => {
                let mid = self.parse_braket_operand(start)?;
                match self.peek() {
                    TokenKind::RAngle => {
                        self.advance();
                        self.braket_inner(start, &bra, &mid)
                    }
                    TokenKind::Pipe => {
                        self.advance();
                        let ket = self.parse_braket_operand(start)?;
                        if !self.eat(&TokenKind::RAngle) {
                            self.error_here("E-SYN-101", "expected `⟩` to close the sandwich");
                            return None;
                        }
                        self.braket_sandwich(start, &bra, &mid, &ket)
                    }
                    other => {
                        self.error_here(
                            "E-SYN-101",
                            format!(
                                "expected `⟩` or `|` in the braket form, found {}",
                                other.describe()
                            ),
                        );
                        None
                    }
                }
            }
            // Standalone bra: end of the operand position.
            _ => self.braket_operand_expr(&bra, start),
        }
    }

    /// fdby: one braket operand — an integer basis label or an
    /// identifier naming a vector. Labels are validated when converted
    /// to values, so `⟨2|…⟩` refuses with the carrier named.
    fn parse_braket_operand(&mut self, start: Span) -> Option<BraketOperand> {
        match self.peek().clone() {
            TokenKind::Int(label) => {
                self.advance();
                Some(BraketOperand::Label(label))
            }
            TokenKind::Ident(name) => {
                self.advance();
                let _ = start;
                Some(BraketOperand::Name(name))
            }
            other => {
                self.error_here(
                    "E-SYN-101",
                    format!(
                        "expected a braket label or name, found {}",
                        other.describe()
                    ),
                );
                None
            }
        }
    }

    /// fdby: the value of one braket operand. A label is the constant
    /// real basis vector on the pack's 2-level carrier; a name is the
    /// named vector itself.
    fn braket_operand_expr(&mut self, operand: &BraketOperand, start: Span) -> Option<Expr> {
        match operand {
            BraketOperand::Label(label) => {
                let entries: [&str; 2] = match label.as_str() {
                    "0" => ["1.0", "0.0"],
                    "1" => ["0.0", "1.0"],
                    other => {
                        self.error_here(
                            "E-SYN-101",
                            format!(
                                "braket label `{other}` is outside the pack's real 2-level \
                                 carrier (|0⟩, |1⟩); a wider carrier (Complex entries, general \
                                 dimension) is a documented follow-up"
                            ),
                        );
                        return None;
                    }
                };
                Some(Expr {
                    kind: ExprKind::List(
                        entries
                            .iter()
                            .map(|text| Expr {
                                kind: ExprKind::Float((*text).into()),
                                source: start,
                            })
                            .collect(),
                    ),
                    source: start.cover(self.last_span()),
                })
            }
            BraketOperand::Name(name) => Some(Expr {
                kind: ExprKind::Path {
                    segments: vec![name.clone()],
                    generics: None,
                },
                source: start.cover(self.last_span()),
            }),
        }
    }

    /// fdby: `⟨φ|ψ⟩` — the inner product. Label×label folds to the
    /// Kronecker delta (`⟨0|1⟩` IS 0, machine-checked at parse);
    /// otherwise the exact desugar is the admitted `dot` builtin, since
    /// sesquilinear conjugation is the identity on the real carrier.
    fn braket_inner(
        &mut self,
        start: Span,
        bra: &BraketOperand,
        ket: &BraketOperand,
    ) -> Option<Expr> {
        if let (BraketOperand::Label(a), BraketOperand::Label(b)) = (bra, ket) {
            let value = if a == b { "1" } else { "0" };
            return Some(Expr {
                kind: ExprKind::Int(value.into()),
                source: start.cover(self.last_span()),
            });
        }
        let bra_expr = self.braket_operand_expr(bra, start)?;
        let ket_expr = self.braket_operand_expr(ket, start)?;
        Some(Expr {
            kind: ExprKind::Call {
                function: Box::new(Expr {
                    kind: ExprKind::Path {
                        segments: vec!["dot".into()],
                        generics: None,
                    },
                    source: start,
                }),
                args: vec![bra_expr, ket_expr],
            },
            source: start.cover(self.last_span()),
        })
    }

    /// fdby: `⟨φ|P|ψ⟩` — the sandwich, as the double sum over the
    /// pack's 2-level carrier: `sum j in 0..2: φ[j] * (sum k in 0..2:
    /// P[j, k] * ψ[k])`. Every piece (sum binder, indexing, scalar
    /// multiply) is an admitted operation; the conjugate is the
    /// identity on the real carrier.
    fn braket_sandwich(
        &mut self,
        start: Span,
        bra: &BraketOperand,
        mid: &BraketOperand,
        ket: &BraketOperand,
    ) -> Option<Expr> {
        let zero = || Expr {
            kind: ExprKind::Int("0".into()),
            source: start,
        };
        let two = || Expr {
            kind: ExprKind::Int("2".into()),
            source: start,
        };
        let carrier = || Expr {
            kind: ExprKind::Range {
                start: Some(Box::new(zero())),
                end: Some(Box::new(two())),
                inclusive: false,
            },
            source: start,
        };
        let binder_expr = |name: &str| Expr {
            kind: ExprKind::Path {
                segments: vec![name.into()],
                generics: None,
            },
            source: start,
        };
        let mid_expr = self.braket_operand_expr(mid, start)?;
        let ket_expr = self.braket_operand_expr(ket, start)?;
        let bra_expr = self.braket_operand_expr(bra, start)?;
        // inner = sum k in 0..2: P[j, k] * ψ[k]
        let inner_body = Expr {
            kind: ExprKind::Binary {
                op: BinaryOp::Mul,
                left: Box::new(Expr {
                    kind: ExprKind::Index {
                        value: Box::new(mid_expr),
                        indices: vec![binder_expr("j"), binder_expr("k")],
                    },
                    source: start.cover(self.last_span()),
                }),
                right: Box::new(Expr {
                    kind: ExprKind::Index {
                        value: Box::new(ket_expr),
                        indices: vec![binder_expr("k")],
                    },
                    source: start.cover(self.last_span()),
                }),
            },
            source: start.cover(self.last_span()),
        };
        let inner = Expr {
            kind: ExprKind::Binder {
                kind: BinderKind::Sum,
                binders: vec![Binder {
                    name: "k".into(),
                    domain: Some(carrier()),
                    source: start,
                }],
                body: Box::new(inner_body),
                guard: None,
            },
            source: start.cover(self.last_span()),
        };
        // outer = sum j in 0..2: φ[j] * inner
        let outer_body = Expr {
            kind: ExprKind::Binary {
                op: BinaryOp::Mul,
                left: Box::new(Expr {
                    kind: ExprKind::Index {
                        value: Box::new(bra_expr),
                        indices: vec![binder_expr("j")],
                    },
                    source: start.cover(self.last_span()),
                }),
                right: Box::new(inner),
            },
            source: start.cover(self.last_span()),
        };
        Some(Expr {
            kind: ExprKind::Binder {
                kind: BinderKind::Sum,
                binders: vec![Binder {
                    name: "j".into(),
                    domain: Some(carrier()),
                    source: start,
                }],
                body: Box::new(outer_body),
                guard: None,
            },
            source: start.cover(self.last_span()),
        })
    }

    /// fdby: `|i⟩⟨j|` — the outer product (projector), the constant
    /// real matrix with 1 at `[i, j]` on the 2-level carrier.
    fn braket_projector(&mut self, start: Span, row: &str, col: &str) -> Option<Expr> {
        let Ok(row_n) = row.parse::<usize>() else {
            self.error_here(
                "E-SYN-101",
                format!(
                    "braket label `{row}` is outside the pack's real 2-level carrier (|0⟩, |1⟩)"
                ),
            );
            return None;
        };
        let Ok(col_n) = col.parse::<usize>() else {
            self.error_here(
                "E-SYN-101",
                format!(
                    "braket label `{col}` is outside the pack's real 2-level carrier (|0⟩, |1⟩)"
                ),
            );
            return None;
        };
        if row_n > 1 || col_n > 1 {
            self.error_here(
                "E-SYN-101",
                format!(
                    "braket label outside the pack's real 2-level carrier (|0⟩, |1⟩): \
                     `|{row}⟩⟨{col}|`; a wider carrier is a documented follow-up"
                ),
            );
            return None;
        }
        let rows = (0..2)
            .map(|r| Expr {
                kind: ExprKind::List(
                    (0..2)
                        .map(|c| Expr {
                            kind: ExprKind::Float(
                                if r == row_n && c == col_n {
                                    "1.0"
                                } else {
                                    "0.0"
                                }
                                .into(),
                            ),
                            source: start,
                        })
                        .collect(),
                ),
                source: start,
            })
            .collect();
        Some(Expr {
            kind: ExprKind::List(rows),
            source: start.cover(self.last_span()),
        })
    }

    /// U8 (emath-r3-string-interp-og2e): validate an interpolated
    /// string template at parse. Purity: a hole carries only a name or
    /// a dotted path — expressions, calls, and indexing refuse, so a
    /// side effect is impossible by grammar, not by discipline. The
    /// format spec is FIXED (`.` digits `f`); `{{`/`}}` escape literal
    /// braces; any other stray brace refuses. The template VALUE stays
    /// raw in the `Str` literal — substitution is the string world's
    /// job, which is outside the Phase 1 subset (every string value
    /// refuses at admission today); this validation is the grammar and
    /// its purity fence, the bead's parse-level contract.
    fn validate_interpolation(&mut self, value: &str, _start: Span) -> Option<()> {
        let chars: Vec<char> = value.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            match chars[i] {
                '{' if chars.get(i + 1) == Some(&'{') => i += 2,
                '}' if chars.get(i + 1) == Some(&'}') => i += 2,
                '{' => {
                    let mut j = i + 1;
                    while j < chars.len() && chars[j] != '}' {
                        j += 1;
                    }
                    if j >= chars.len() {
                        self.error_here(
                            "E-SYN-101",
                            "interpolation hole is never closed; expected `}`",
                        );
                        return None;
                    }
                    let hole: String = chars[i + 1..j].iter().collect();
                    self.validate_hole(&hole)?;
                    i = j + 1;
                }
                '}' => {
                    self.error_here(
                        "E-SYN-101",
                        "unescaped `}` in string; write `}}` for a literal brace",
                    );
                    return None;
                }
                _ => i += 1,
            }
        }
        Some(())
    }

    /// U8: one interpolation hole — `name`, `dotted.path`, each with an
    /// optional fixed format spec `.Nf`.
    fn validate_hole(&mut self, hole: &str) -> Option<()> {
        let (path, spec) = match hole.split_once(':') {
            Some((path, spec)) => (path, Some(spec)),
            None => (hole, None),
        };
        let pure = !path.is_empty()
            && path
                .chars()
                .next()
                .is_some_and(|c| c.is_alphabetic() || c == '_')
            && path
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '.')
            && !path.split('.').any(|segment| segment.is_empty());
        if !pure {
            self.error_here(
                "E-SYN-101",
                format!(
                    "interpolation hole `{{{hole}}}` carries an expression; holes carry \
                     only names or paths (purity — no side effects in the report lane)"
                ),
            );
            return None;
        }
        if let Some(spec) = spec {
            let bytes = spec.as_bytes();
            let spec_ok = bytes.len() >= 3
                && bytes[0] == b'.'
                && bytes[1..bytes.len() - 1]
                    .iter()
                    .all(|byte| byte.is_ascii_digit())
                && bytes[bytes.len() - 1] == b'f';
            if !spec_ok {
                self.error_here(
                    "E-SYN-101",
                    format!(
                        "interpolation spec `{{{hole}}}` is not the fixed format spec \
                         `.Nf` (e.g. `{{x:.3f}}`); arbitrary format strings refuse"
                    ),
                );
                return None;
            }
        }
        Some(())
    }

    /// Parse a compound-unit bracket `[unit m/s^2]` (F7/U4); the `unit`
    /// keyword disambiguates from indexing. `None` when not a unit bracket.
    fn parse_unit_bracket(&mut self, depth: usize) -> Option<UnitExpr> {
        // Only enter if the next token is `[`.
        if !matches!(self.peek(), TokenKind::LBracket) {
            return None;
        }
        // Peek ahead: `[` must be followed by the identifier `unit`.
        let peek1 = self.peek_at(1).clone();
        if !matches!(peek1, TokenKind::Ident(name) if name == "unit") {
            return None;
        }
        // Consume `[` and `unit`.
        self.advance();
        self.advance();

        let unit_expr = self.parse_unit_expr(depth)?;

        if !self.eat(&TokenKind::RBracket) {
            self.error_here("E-SYN-102", "expected `]` to close unit bracket");
            return None;
        }
        Some(unit_expr)
    }

    /// Parse a unit expression: `m/s^2`, `kg*m^2/s^2`, `m/(s*s)`.
    /// Left-associative for `*` and `/` (C2 trap: `m/s*s` = length, not acceleration).
    fn parse_unit_expr(&mut self, depth: usize) -> Option<UnitExpr> {
        let _ = depth;
        let mut left = self.parse_unit_atom()?;
        loop {
            match self.peek() {
                TokenKind::Star => {
                    self.advance();
                    let right = self.parse_unit_atom()?;
                    left = UnitExpr::Mul(Box::new(left), Box::new(right));
                }
                TokenKind::Slash => {
                    self.advance();
                    let right = self.parse_unit_atom()?;
                    left = UnitExpr::Div(Box::new(left), Box::new(right));
                }
                _ => break,
            }
        }
        Some(left)
    }

    /// Parse a unit atom: identifier, parenthesized group, or power.
    fn parse_unit_atom(&mut self) -> Option<UnitExpr> {
        let base = match self.peek().clone() {
            TokenKind::Ident(name) => {
                self.advance();
                UnitExpr::Base(name)
            }
            TokenKind::LParen => {
                self.advance();
                let inner = self.parse_unit_expr(0)?;
                if !self.eat(&TokenKind::RParen) {
                    self.error_here("E-SYN-101", "expected `)` to close unit group");
                    return None;
                }
                inner
            }
            other => {
                self.error_here(
                    "E-SYN-101",
                    format!("expected unit name, found {}", other.describe()),
                );
                return None;
            }
        };
        // Check for power: `s^2`
        if matches!(self.peek(), TokenKind::Caret) {
            self.advance();
            let TokenKind::Int(exp_str) = self.peek().clone() else {
                self.error_here("E-SYN-101", "expected integer exponent after `^`");
                return None;
            };
            self.advance();
            let exp: i32 = exp_str.parse().unwrap_or(1);
            return Some(UnitExpr::Pow(Box::new(base), exp));
        }
        Some(base)
    }
}

/// One U6 match pattern (see [`Parser::parse_match_pattern`]).
enum MatchPattern {
    /// A literal pattern; becomes a `subject == literal` arm condition.
    Literal(Expr),
    /// `_` (no binding) or a binding name; must be the final arm and
    /// becomes the cases else arm. A binding name has the subject
    /// substituted for it in the arm value.
    CatchAll(Option<String>),
}

/// One fdby braket operand: an integer basis label (`|0⟩`, `⟨1|`) or an
/// identifier naming a vector (`⟨psi|`).
enum BraketOperand {
    Label(String),
    Name(String),
}

/// Substitute the match subject for a binding-pattern name in an arm
/// value (U6). Lexical scoping is honored: a binder, comprehension
/// variable, or limit variable of the same name shadows the binding, so
/// its body/guard is left untouched while its domain/target (evaluated
/// outside the binder) is still substituted. The declared tolerance
/// clause of `≈` is numeric-literal territory and is left untouched.
fn substitute_bound(expr: Expr, subject: &Expr, name: &str) -> Expr {
    let source = expr.source;
    let kind = match expr.kind {
        ExprKind::Path { segments, generics }
            if generics.is_none() && segments.len() == 1 && segments[0] == name =>
        {
            return Expr {
                kind: subject.kind.clone(),
                source,
            };
        }
        ExprKind::Quantity { value, unit } => ExprKind::Quantity {
            value: Box::new(substitute_bound(*value, subject, name)),
            unit,
        },
        ExprKind::Call { function, args } => ExprKind::Call {
            function: Box::new(substitute_bound(*function, subject, name)),
            args: args
                .into_iter()
                .map(|arg| substitute_bound(arg, subject, name))
                .collect(),
        },
        ExprKind::Index { value, indices } => ExprKind::Index {
            value: Box::new(substitute_bound(*value, subject, name)),
            indices: indices
                .into_iter()
                .map(|index| substitute_bound(index, subject, name))
                .collect(),
        },
        ExprKind::Slice { start, end } => ExprKind::Slice {
            start: start.map(|e| Box::new(substitute_bound(*e, subject, name))),
            end: end.map(|e| Box::new(substitute_bound(*e, subject, name))),
        },
        ExprKind::Unary { op, value } => ExprKind::Unary {
            op,
            value: Box::new(substitute_bound(*value, subject, name)),
        },
        ExprKind::Binary { op, left, right } => ExprKind::Binary {
            op,
            left: Box::new(substitute_bound(*left, subject, name)),
            right: Box::new(substitute_bound(*right, subject, name)),
        },
        ExprKind::Approx {
            left,
            right,
            tolerance,
        } => ExprKind::Approx {
            left: Box::new(substitute_bound(*left, subject, name)),
            right: Box::new(substitute_bound(*right, subject, name)),
            tolerance,
        },
        ExprKind::If {
            condition,
            then_value,
            else_value,
        } => ExprKind::If {
            condition: Box::new(substitute_bound(*condition, subject, name)),
            then_value: Box::new(substitute_bound(*then_value, subject, name)),
            else_value: Box::new(substitute_bound(*else_value, subject, name)),
        },
        ExprKind::List(items) => ExprKind::List(
            items
                .into_iter()
                .map(|item| substitute_bound(item, subject, name))
                .collect(),
        ),
        ExprKind::Table { headers, rows } => ExprKind::Table {
            headers,
            rows: rows
                .into_iter()
                .map(|row| {
                    row.into_iter()
                        .map(|cell| substitute_bound(cell, subject, name))
                        .collect()
                })
                .collect(),
        },
        ExprKind::Set(items) => ExprKind::Set(
            items
                .into_iter()
                .map(|item| substitute_bound(item, subject, name))
                .collect(),
        ),
        ExprKind::SetComprehension {
            element,
            var,
            domain,
            guard,
        } => {
            let shadowed = var == name;
            ExprKind::SetComprehension {
                element: if shadowed {
                    element
                } else {
                    Box::new(substitute_bound(*element, subject, name))
                },
                var,
                domain: Box::new(substitute_bound(*domain, subject, name)),
                guard: if shadowed {
                    guard
                } else {
                    guard.map(|e| Box::new(substitute_bound(*e, subject, name)))
                },
            }
        }
        ExprKind::Record { type_path, fields } => ExprKind::Record {
            type_path,
            fields: fields
                .into_iter()
                .map(|(field, value)| (field, substitute_bound(value, subject, name)))
                .collect(),
        },
        ExprKind::Tuple(items) => ExprKind::Tuple(
            items
                .into_iter()
                .map(|item| substitute_bound(item, subject, name))
                .collect(),
        ),
        ExprKind::Range {
            start,
            end,
            inclusive,
        } => ExprKind::Range {
            start: start.map(|e| Box::new(substitute_bound(*e, subject, name))),
            end: end.map(|e| Box::new(substitute_bound(*e, subject, name))),
            inclusive,
        },
        ExprKind::Binder {
            kind,
            binders,
            body,
            guard,
        } => {
            let shadowed = binders.iter().any(|binder| binder.name == name);
            ExprKind::Binder {
                kind,
                binders: binders
                    .into_iter()
                    .map(|binder| Binder {
                        domain: binder
                            .domain
                            .map(|domain| substitute_bound(domain, subject, name)),
                        ..binder
                    })
                    .collect(),
                body: if shadowed {
                    body
                } else {
                    Box::new(substitute_bound(*body, subject, name))
                },
                guard: if shadowed {
                    guard
                } else {
                    guard.map(|e| Box::new(substitute_bound(*e, subject, name)))
                },
            }
        }
        ExprKind::Derivative {
            value,
            wrt,
            kind,
            holding,
        } => ExprKind::Derivative {
            value: Box::new(substitute_bound(*value, subject, name)),
            wrt: wrt.map(|list| {
                list.into_iter()
                    .map(|e| substitute_bound(e, subject, name))
                    .collect()
            }),
            kind,
            holding: holding
                .into_iter()
                .map(|e| substitute_bound(e, subject, name))
                .collect(),
        },
        ExprKind::Solve { value, wrt } => ExprKind::Solve {
            value: Box::new(substitute_bound(*value, subject, name)),
            wrt: wrt.map(|list| {
                list.into_iter()
                    .map(|e| substitute_bound(e, subject, name))
                    .collect()
            }),
        },
        ExprKind::Optimize {
            value,
            wrt,
            maximize,
        } => ExprKind::Optimize {
            value: Box::new(substitute_bound(*value, subject, name)),
            wrt: wrt.map(|list| {
                list.into_iter()
                    .map(|e| substitute_bound(e, subject, name))
                    .collect()
            }),
            maximize,
        },
        ExprKind::At { value, location } => ExprKind::At {
            value: Box::new(substitute_bound(*value, subject, name)),
            location: Box::new(substitute_bound(*location, subject, name)),
        },
        ExprKind::On { value, location } => ExprKind::On {
            value: Box::new(substitute_bound(*value, subject, name)),
            location: Box::new(substitute_bound(*location, subject, name)),
        },
        ExprKind::Conditioned { value, condition } => ExprKind::Conditioned {
            value: Box::new(substitute_bound(*value, subject, name)),
            condition: Box::new(substitute_bound(*condition, subject, name)),
        },
        ExprKind::UnitQuery { kind, expr } => ExprKind::UnitQuery {
            kind,
            expr: Box::new(substitute_bound(*expr, subject, name)),
        },
        ExprKind::Limit {
            var,
            target,
            direction,
            body,
        } => {
            let shadowed = var == name;
            ExprKind::Limit {
                var,
                target: Box::new(substitute_bound(*target, subject, name)),
                direction,
                body: if shadowed {
                    body
                } else {
                    Box::new(substitute_bound(*body, subject, name))
                },
            }
        }
        ExprKind::SampleLimit {
            var,
            target,
            direction,
            body,
        } => {
            let shadowed = var == name;
            ExprKind::SampleLimit {
                var,
                target: Box::new(substitute_bound(*target, subject, name)),
                direction,
                body: if shadowed {
                    body
                } else {
                    Box::new(substitute_bound(*body, subject, name))
                },
            }
        }
        ExprKind::Cases {
            subject: inner,
            arms,
            else_arm,
        } => ExprKind::Cases {
            subject: inner.map(|e| Box::new(substitute_bound(*e, subject, name))),
            arms: arms
                .into_iter()
                .map(|(condition, value)| {
                    (
                        substitute_bound(condition, subject, name),
                        substitute_bound(value, subject, name),
                    )
                })
                .collect(),
            else_arm: Box::new(substitute_bound(*else_arm, subject, name)),
        },
        other => other,
    };
    Expr { kind, source }
}
