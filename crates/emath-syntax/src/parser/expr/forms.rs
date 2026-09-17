//! Compound body forms: limit, cases, match, graph literals; match patterns; bound substitution.

use super::*;

impl super::super::Parser {
    /// B04: Parse the body of a `limit x -> T[+|-]: body` expression; the
    /// target parses at multiplicative level so `+`/`-` before `:` is a
    /// direction suffix (complex targets need parens).
    #[allow(dead_code)]
    pub(super) fn parse_limit_body(
        &mut self,
        start: emath_core::Span,
        var: String,
        is_sample: bool,
        depth: usize,
    ) -> Option<Expr> {
        let _ = (start, var, is_sample, depth);
        self.error_here(
            "E-SYN-101",
            "`limit` / `sample_limit` are ordinary imported functions, not constructor forms",
        );
        None
    }

    /// U1: Parse a `cases [subject]: | c1 => e1 | else => eN` body; arms use
    /// `|`/`=>` and a mandatory `else` arm enforces totality.
    pub(super) fn parse_cases_body(
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
                    return None;
                }
                if subject.is_some() {
                    else_arm = Some(Box::new(Expr {
                        kind: ExprKind::Path {
                            segments: vec!["_unmatched".into()],
                            generics: None,
                        },
                        source: self.current_span(),
                    }));
                    break;
                }
                self.error_here(
                    "E-SYN-110",
                    "cases expression requires a mandatory `else` arm",
                );
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
    pub(super) fn parse_match_body(
        &mut self,
        start: Span,
        subject: Box<Expr>,
        depth: usize,
    ) -> Option<Expr> {
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
    pub(super) fn parse_match_pattern(&mut self, _depth: usize) -> Option<MatchPattern> {
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

}

/// One U6 match pattern (see [`Parser::parse_match_pattern`]).
pub(super) enum MatchPattern {
    /// A literal pattern; becomes a `subject == literal` arm condition.
    Literal(Expr),
    /// `_` (no binding) or a binding name; must be the final arm and
    /// becomes the cases else arm. A binding name has the subject
    /// substituted for it in the arm value.
    CatchAll(Option<String>),
}

/// One braket operand: an integer basis label (`|0⟩`, `⟨1|`) or an

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
        ExprKind::Conditioned { value, condition } => ExprKind::Conditioned {
            value: Box::new(substitute_bound(*value, subject, name)),
            condition: Box::new(substitute_bound(*condition, subject, name)),
        },
        ExprKind::UnitQuery { kind, expr } => ExprKind::UnitQuery {
            kind,
            expr: Box::new(substitute_bound(*expr, subject, name)),
        },
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
