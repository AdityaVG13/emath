use crate::token::{Keyword, TokenKind};
use crate::tree::{BinaryOp, Expr, ExprKind, ReactionArrow, ReactionTerm, Stmt, StmtKind, Suite};
use emath_core::Span;

impl super::Parser {
    pub(super) fn parse_suite(&mut self) -> Option<Suite> {
        self.parse_suite_inner(false, false)
    }

    /// `example <name>:` with no indented body is a worked example, not
    /// `E-SYN-112`. Other section heads still require a block. The section
    /// name selects the suite's line grammar: `reactions:` parses T3
    /// reaction lines instead of expression statements (04 section 3.1).
    pub(super) fn parse_section_suite(&mut self, section_name: &str) -> Option<Suite> {
        self.parse_suite_inner(section_name == "example", section_name == "reactions")
    }

    fn parse_suite_inner(&mut self, allow_empty: bool, reactions: bool) -> Option<Suite> {
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
                let parsed = if reactions {
                    self.parse_reaction_line_stmt()
                } else {
                    self.parse_statement()
                };
                match parsed {
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

    /// One `reactions:` line: `name: 2H2 + O2 -> 2H2O`. Terms are
    /// (coefficient, species) pairs — T3 section grammar, never the
    /// expression grammar, so `2H2` is a term here while juxtaposition
    /// stays refused in expressions (C15). Arrow spellings: `->`
    /// (irreversible), `<->` (reversible), `<=>` (equilibrium). Any other
    /// arrow spelling refuses E-SYN-156.
    fn parse_reaction_line_stmt(&mut self) -> Option<Stmt> {
        let start = self.current_span();
        let TokenKind::Ident(name) = self.peek().clone() else {
            return self.parse_statement();
        };
        if !matches!(self.peek_at(1), TokenKind::Colon) {
            // Not a `name:` line — fall back to the ordinary statement
            // parser so its diagnostics stay in charge.
            return self.parse_statement();
        }
        self.advance(); // name
        self.advance(); // :
        let lhs = self.parse_reaction_terms()?;
        let arrow = self.parse_reaction_arrow()?;
        let rhs = self.parse_reaction_terms()?;
        // 04 §4.1: a side that is nothing
        // must be DECLARED nothing — `∅` is the sink spelling. A
        // reaction endpoint that is empty without it is a silent
        // nothing and refuses at admission (E-BIO-SINK).
        if lhs.is_empty() && rhs.is_empty() {
            self.error_here(
                "E-SYN-156",
                "both sides of the reaction are empty; a sink endpoint must be the declared \
                 sink `∅` (04 §4.1), never a silently empty side",
            );
            return None;
        }
        // The line must end after the RHS terms.
        if !matches!(
            self.peek(),
            TokenKind::Newline | TokenKind::Dedent | TokenKind::Eof
        ) {
            self.error_here(
                "E-SYN-156",
                "unexpected trailing tokens after the reaction products; a `reactions:` line is \
                 `name: terms <arrow> terms` with arrows `->`, `<->`, `<=>`",
            );
            return None;
        }
        Some(self.stmt(
            start,
            StmtKind::Reaction {
                name,
                lhs,
                arrow,
                rhs,
            },
        ))
    }

    /// Terms of one reaction side: `term ('+' term)*`, coefficient
    /// optional (default 1). Stops at the arrow or end of line. The
    /// declared sink `∅` (04 §4.1) is a legal side BY ITSELF
    /// (degradation/elimination: `Drug -> ∅`); a side that is neither
    /// terms nor `∅` (a bare empty side) refuses E-SYN-156 here and
    /// E-BIO-SINK at admission — an endpoint that is nothing must be
    /// declared nothing, never silently empty.
    fn parse_reaction_terms(&mut self) -> Option<Vec<ReactionTerm>> {
        let mut terms = Vec::new();
        // Declared sink: a bare `∅` IS the side.
        if matches!(self.peek(), TokenKind::EmptySet) {
            self.advance();
            return Some(Vec::new());
        }
        loop {
            let coefficient = match self.peek().clone() {
                TokenKind::Int(text) => {
                    let Ok(value) = text.parse::<u64>() else {
                        self.error_here(
                            "E-SYN-156",
                            format!("coefficient `{text}` is not a non-negative integer"),
                        );
                        return None;
                    };
                    self.advance();
                    value
                }
                _ => 1,
            };
            let TokenKind::Ident(species) = self.peek().clone() else {
                self.error_here(
                    "E-SYN-156",
                    "expected a species name after the coefficient (reaction lines are \
                     `coefficient species` pairs joined by `+`)",
                );
                return None;
            };
            self.advance();
            terms.push(ReactionTerm {
                coefficient,
                species,
            });
            if !self.eat(&TokenKind::Plus) {
                break;
            }
        }
        Some(terms)
    }

    /// The arrow between the two sides. Lexer reality (04 section 3.1):
    /// `->` and `=>` share one Arrow token (notation mappings depend on
    /// that), and `<->` lexes as `<` + Arrow while `<=>` lexes as
    /// `<` + `<=` + `>`. So inside `reactions:` the admitted arrows are
    /// Arrow (irreversible; both `->` and `=>` spellings denote it — the
    /// grammar has no lambda position for `=>` to desugar into), `<`+Arrow
    /// (reversible), `<`+`<=`+`>` (equilibrium). `<==>` is the logical
    /// Iff token and refuses, as does any other spelling.
    fn parse_reaction_arrow(&mut self) -> Option<ReactionArrow> {
        if self.eat(&TokenKind::Arrow) {
            return Some(ReactionArrow::Irreversible);
        }
        // `<=>` lexes as `<=` + `>` (the lexer prefers `<=`; `<==>` is the
        // logical Iff token and is NOT an admitted reaction arrow).
        if self.eat(&TokenKind::Le) && self.eat(&TokenKind::Gt) {
            return Some(ReactionArrow::Equilibrium);
        }
        // `<->` lexes as `<` + `->`: the `-` is followed by `>`, which the
        // lexer folds into the same Arrow token `->` and `=>` share.
        if self.eat(&TokenKind::Lt) && self.eat(&TokenKind::Arrow) {
            return Some(ReactionArrow::Reversible);
        }
        self.error_here(
            "E-SYN-156",
            "unknown arrow spelling; the admitted arrows are `->`, `<->`, `<=>`",
        );
        None
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
