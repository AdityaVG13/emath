//! Series/policy suffixes and segment statements.

use super::*;

impl super::super::Parser {
    /// 04 §5.4: optional series-policy suffix
    /// on a definition/observation value — `with interpolation: <mode>
    /// [, extrapolation: <mode>]`. The policy is part of the value's
    /// identity; an absent part means the language default (`refuse` for
    /// extrapolation, declared-required interpolation handled by
    /// admission). Only valid in row-value position, never mid-expression.
    pub(super) fn parse_series_policy_suffix(&mut self, value: Expr, start: Span) -> Option<Expr> {
        if !matches!(self.peek(), TokenKind::Keyword(Keyword::With)) {
            return Some(value);
        }
        self.advance(); // with
        let mut interpolation = None;
        let mut extrapolation = None;
        loop {
            let TokenKind::Ident(key) = self.peek().clone() else {
                self.error_here(
                    "E-SYN-101",
                    "expected `interpolation` or `extrapolation` after `with`",
                );
                return None;
            };
            self.advance();
            if !self.eat(&TokenKind::Colon) {
                self.error_here("E-SYN-111", "expected `:` after `with interpolation`");
                return None;
            }
            let TokenKind::Ident(mode) = self.peek().clone() else {
                self.error_here("E-SYN-101", "expected a mode name after the policy key");
                return None;
            };
            self.advance();
            match key.as_str() {
                "interpolation" => {
                    if interpolation.is_some() {
                        self.error_here("E-SYN-103", "duplicate `interpolation` policy");
                        return None;
                    }
                    interpolation = Some(match mode.as_str() {
                        "previous" => SeriesInterpolation::Previous,
                        "linear" => SeriesInterpolation::Linear,
                        "nearest" => SeriesInterpolation::Nearest,
                        "pwc" => SeriesInterpolation::Pwc,
                        "monotone_cubic" => SeriesInterpolation::MonotoneCubic,
                        other => {
                            self.error_here(
                                "E-SYN-101",
                                format!(
                                    "unknown interpolation mode `{other}` (known: previous, \
                                     linear, nearest, pwc, monotone_cubic)"
                                ),
                            );
                            return None;
                        }
                    });
                }
                "extrapolation" => {
                    if extrapolation.is_some() {
                        self.error_here("E-SYN-103", "duplicate `extrapolation` policy");
                        return None;
                    }
                    extrapolation = Some(match mode.as_str() {
                        "refuse" => SeriesExtrapolation::Refuse,
                        "clamp" => SeriesExtrapolation::Clamp,
                        "extend" => SeriesExtrapolation::Extend,
                        other => {
                            self.error_here(
                                "E-SYN-101",
                                format!(
                                    "unknown extrapolation mode `{other}` (known: refuse, \
                                     clamp, extend)"
                                ),
                            );
                            return None;
                        }
                    });
                }
                other => {
                    self.error_here(
                        "E-SYN-101",
                        format!(
                            "unknown series policy `{other}` (known: interpolation, \
                             extrapolation)"
                        ),
                    );
                    return None;
                }
            }
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        Some(Expr {
            kind: ExprKind::WithSeriesPolicy {
                value: Box::new(value),
                interpolation,
                extrapolation,
            },
            source: start.cover(self.last_span()),
        })
    }

    /// Shared tail for collected segment runs: `name = value` assignment,
    /// multi-word `head = value` command, or plain command. An ident-led
    /// `≈` claim (`y ≈ rhs within ...`) is an expression statement, not a
    /// command (04 §6.4).
    pub(super) fn finish_segments_statement(
        &mut self,
        start: Span,
        segments: Vec<String>,
        via_dot: bool,
    ) -> Option<Stmt> {
        if matches!(self.peek(), TokenKind::TildeEq) && !via_dot && segments.len() == 1 {
            let left = Expr {
                kind: ExprKind::Path {
                    segments,
                    generics: None,
                },
                source: start.cover(self.last_span()),
            };
            let expr = self.parse_approx_tail(left)?;
            return Some(self.stmt(start, StmtKind::Expr(expr)));
        }
        if self.eat(&TokenKind::Eq) {
            let opened = self.skip_assignment_layout();
            // 04 §2.5: the action
            // integral binder on the RHS of a definition
            // (`S = action integral t in t0..t1: L(...)`) — refuse at
            // the grammar naming the design follow-up (previously the
            // row half-parsed: `unknown variable action` plus a generic
            // row-shape error that named nothing).
            if matches!(
                self.peek(),
                TokenKind::Ident(word) if word == "action"
            ) && matches!(self.peek_at(1), TokenKind::Keyword(Keyword::Integral))
            {
                while !matches!(
                    self.peek(),
                    TokenKind::Newline | TokenKind::Dedent | TokenKind::Eof
                ) {
                    self.advance();
                }
                self.error_here(
                    "E-SYN-101",
                    "`action integral t in t0..t1: L(...)` is outside the Phase 1 subset — the action/variation design follow-up must first settle the design of record: the action is a Functional (admits only variation/evaluation goals, never scalar composition), the `variation <S> wrt q:` goal lowers to core goals via the Euler-Lagrange operator built from the admitted partial/total derivatives, and the boundary condition (`fixed_endpoints`) is part of the goal identity hash; evidence must use admitted surface `derivative(derivative(q) wrt t) wrt t`, never `d²q/dt²` (C14)",
                );
                return None;
            }
            let value = self.parse_expr()?;
            let value = self.parse_series_policy_suffix(value, start)?;
            self.close_assignment_indent(opened);
            if segments.len() == 1 || via_dot {
                return Some(self.stmt(
                    start,
                    StmtKind::Assign {
                        target: Place {
                            segments,
                            indices: vec![],
                            source: start.cover(self.last_span()),
                        },
                        value,
                    },
                ));
            }
            // `budget iterations = N` command with value
            return Some(self.stmt(
                start,
                StmtKind::Command {
                    head: segments,
                    argument: Some(CommandArgument::Expr(value)),
                },
            ));
        }
        // Unit-carrying row (`name in unit = value`): outside the Phase 1
        // subset. Previously this row parsed as a command and was then
        // dropped by admission without a diagnostic (only a downstream
        // unknown-variable error appeared). Refuse at the grammar, loudly.
        // Keyword-led binders (`sum i in a..b:`, `series X in ...`) and
        // colon-carrying typed rows (`x: Float64 in s`) never reach here.
        if matches!(self.peek(), TokenKind::Keyword(Keyword::In)) {
            self.error_here(
                "E-SYN-101",
                "definition rows do not carry units: `name in unit = value` is outside the Phase 1 subset; put the unit on the quantity (`0.30 [unit 1/day]`) or on the declared input/output row (`k: Float64 in 1/day`); unit-carrying measured rate parameters are the bio dynamics field-pack follow-up",
            );
            return None;
        }
        let (head, argument) = self.parse_command_tail(segments)?;
        Some(self.stmt(start, StmtKind::Command { head, argument }))
    }
}
