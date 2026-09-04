//! The default identifier-statement path.

use super::*;

impl super::super::Parser {
    /// Generic ident-headed statement: sections, fields, commands, assigns.
    pub(super) fn parse_default_ident_statement(&mut self, start: Span) -> Option<Stmt> {
        let TokenKind::Ident(name) = self.peek().clone() else {
            return None;
        };

        // `evaluate <score>:` section heads: but not comparisons
        // (`a < b < c`): the matching `>` must be followed by `:` or `(`.
        if matches!(self.peek_at(1), TokenKind::Lt) && self.lookahead_matches_lt_angle_head() {
            self.advance(); // name
            self.advance(); // <
            let TokenKind::Ident(generic) = self.peek().clone() else {
                self.error_here("E-SYN-101", "expected a name inside `< >`");
                return None;
            };
            self.advance();
            if !self.eat(&TokenKind::Gt) {
                self.error_here("E-SYN-102", "expected `>` to close section head");
                return None;
            }
            let args = if matches!(self.peek(), TokenKind::LParen) {
                if let Some(args) = self.parse_arguments() {
                    Some(args)
                } else {
                    self.error_here("E-SYN-101", "malformed argument list in section head");
                    return None;
                }
            } else {
                None
            };
            if !self.eat(&TokenKind::Colon) {
                self.error_here("E-SYN-111", "expected `:` after section head");
                return None;
            }
            let suite = self.parse_section_suite(&name)?;
            return Some(self.stmt(
                start,
                StmtKind::Section(Section {
                    name,
                    generic: Some(generic),
                    args,
                    suite,
                    source: start.cover(self.last_span()),
                    head_source: start.cover(self.last_span()),
                }),
            ));
        }

        // Three-word constructor-level heads (spec 09 / `emath-nko`):
        // handled as a top-level arm in `parse_ident_statement` above;
        // this default path no longer sees `world`/`artifact` + `constructor`.

        // Two-word section heads: `goal rust:`, `tune score:`,
        // `lower declaration:`, `dispatch authority:`.
        if matches!(self.peek_at(1), TokenKind::Ident(_))
            && matches!(self.peek_at(2), TokenKind::Colon)
        {
            self.advance(); // first word
            let TokenKind::Ident(generic) = self.peek().clone() else {
                return None;
            };
            self.advance();
            if !self.eat(&TokenKind::Colon) {
                self.error_here("E-SYN-111", "expected `:` after section head");
                return None;
            }
            let suite = self.parse_section_suite(&name)?;
            return Some(self.stmt(
                start,
                StmtKind::Section(Section {
                    name,
                    generic: Some(generic),
                    args: None,
                    suite,
                    source: start.cover(self.last_span()),
                    head_source: start.cover(self.last_span()),
                }),
            ));
        }

        // `method score(candidate: ...) -> f64:` two-word fn declarations
        if matches!(self.peek_at(1), TokenKind::Ident(_))
            && matches!(self.peek_at(2), TokenKind::LParen)
        {
            // disambiguate from `produce rust.library` style by scanning for a
            // matching `)` followed by `->` or `:`
            if self.looks_like_params_header() {
                self.advance(); // first word
                let TokenKind::Ident(second) = self.peek().clone() else {
                    return None;
                };
                self.advance();
                let (params, ret) = self.parse_params_after_name_flag(true)?;
                let suite = if self.eat(&TokenKind::Colon) {
                    self.parse_suite()
                } else {
                    None
                };
                return Some(self.stmt(
                    start,
                    StmtKind::FnDecl {
                        visibility: None,
                        head: name,
                        name: second,
                        params,
                        ret,
                        suite,
                        source: start.cover(self.last_span()),
                    },
                ));
            }
        }

        // `candidate(candidate: &CacheCandidate) -> f64:` call or fn?: a
        // single ident + `(` that scans as a params header is a fn decl;
        // otherwise it is an expression statement.
        if matches!(self.peek_at(1), TokenKind::LParen) && self.looks_like_params_header() {
            self.advance();
            let (params, ret) = self.parse_params_after_name()?;
            let suite = if self.eat(&TokenKind::Colon) {
                self.parse_suite()
            } else {
                None
            };
            return Some(self.stmt(
                start,
                StmtKind::FnDecl {
                    visibility: None,
                    head: "fn".to_string(),
                    name,
                    params,
                    ret,
                    suite,
                    source: start.cover(self.last_span()),
                },
            ));
        }

        // Full-expression statements and equations (`equation:` /
        // `constraint:` sections): `mass * derivative(velocity) = rhs`,
        // `a * a + b * b = c * c`, `a < b < c`. Trigger: the token after
        // the leading ident is an operator or a call paren, or a dotted /
        // `::` path continuation that still has an operator or `=` ahead
        // (`core.policy:` is a section head, not an expression).
        // Bare `name = value` stays an assignment.
        let dashed_compile = name == "error"
            && matches!(self.peek_at(1), TokenKind::Minus)
            && matches!(self.peek_at(2), TokenKind::Ident(tail) if tail == "limit");
        let op_led = !dashed_compile
            && matches!(
                self.peek_at(1),
                TokenKind::Star
                    | TokenKind::Slash
                    | TokenKind::Plus
                    | TokenKind::Minus
                    | TokenKind::Caret
                    | TokenKind::Le
                    | TokenKind::Ge
                    | TokenKind::Lt
                    | TokenKind::Gt
                    | TokenKind::EqEq
                    | TokenKind::NotEq
                    | TokenKind::LParen
            );
        let dot_led = matches!(self.peek_at(1), TokenKind::Dot | TokenKind::PathSep)
            && self.dotted_continues_expression();
        if op_led || dot_led {
            let left = self.parse_expr()?;
            if let Some(stmt) = self.parse_equation_tail(&left, start) {
                return Some(stmt);
            }
            return Some(self.stmt(start, StmtKind::Expr(left)));
        }

        // `implement cache_core::Policy for Self:` host binding
        if name == "implement" {
            self.advance();
            let segments = self.collect_segments_with_dots().0;
            if segments.is_empty() {
                self.error_here("E-SYN-110", "expected a path after `implement`");
                return None;
            }
            let mut target = String::new();
            if self.eat_keyword(Keyword::For) {
                target = self.collect_segments_with_dots().0.join("::");
            }
            if !self.eat(&TokenKind::Colon) {
                self.error_here("E-SYN-111", "expected `:` after `implement` head");
                return None;
            }
            let suite = self.parse_suite()?;
            let generic = if target.is_empty() {
                // `implement <path> :` without a `for` target: record the
                // path alone (no trailing separator poisoning the
                // round-trip generic).
                segments.join("::")
            } else {
                format!("{}::{}", segments.join("::"), target)
            };
            return Some(self.stmt(
                start,
                StmtKind::Section(Section {
                    name: "implement".into(),
                    generic: Some(generic),
                    args: None,
                    suite,
                    source: start.cover(self.last_span()),
                    head_source: start.cover(self.last_span()),
                }),
            ));
        }

        // ident `:` section / key-value / field declaration
        if matches!(self.peek_at(1), TokenKind::Colon) {
            match self.peek_at(2).clone() {
                TokenKind::Newline | TokenKind::Indent | TokenKind::Eof => {
                    self.advance(); // name
                    self.advance(); // :
                    let suite = self.parse_section_suite(&name)?;
                    return Some(self.stmt(
                        start,
                        StmtKind::Section(Section {
                            name,
                            generic: None,
                            args: None,
                            suite,
                            source: start.cover(self.last_span()),
                            head_source: start.cover(self.last_span()),
                        }),
                    ));
                }
                TokenKind::Str(_) | TokenKind::Int(_) | TokenKind::Float(_) => {
                    self.advance(); // name
                    self.advance(); // :
                    self.skip_assignment_layout();
                    let value = self.parse_expr()?;
                    return Some(self.stmt(
                        start,
                        StmtKind::Command {
                            head: vec![name],
                            argument: Some(CommandArgument::Expr(value)),
                        },
                    ));
                }
                _ => {
                    self.advance(); // name
                    self.advance(); // :
                    if let Some(ty) = self.parse_type_expr() {
                        let default = if self.eat(&TokenKind::Eq) {
                            self.skip_assignment_layout();
                            self.parse_expr()
                        } else {
                            None
                        };
                        // 05 §7.1: the
                        // inline where-refinement row (`p: Float64 where
                        // 0 <= self and self <= 1`) — the refinement
                        // seed's grammar is a design follow-up; refuse at
                        // the grammar naming the design contract
                        // (previously the row half-parsed and the
                        // dangling `where ...` predicate died with a
                        // generic row-shape error that named nothing).
                        // The ADMITTED refinement surface today is the
                        // domain annotation `Type in [lo, hi]` (U5).
                        if matches!(self.peek(), TokenKind::Keyword(Keyword::Where)) {
                            while !matches!(
                                self.peek(),
                                TokenKind::Newline | TokenKind::Dedent | TokenKind::Eof
                            ) {
                                self.advance();
                            }
                            self.error_here(
                                "E-SYN-101",
                                "`name: Type where <predicate>` refinement rows are outside the Phase 1 subset — the refinement-types design follow-up must first settle: predicates stay in a total, decidable fragment (same discipline as ch8's bounded lowering language; type checking deterministic and budgeted), refinements are recorded in semantic identity (as domain annotations already are), and conflicting constraints are NAMED in diagnostics (ch16 gate 6) so refinements stay usable rather than feared; the admitted refinement surface today is the domain annotation `Type in [lo, hi]`; there is no trust-me cast — the runtime-checked cast downgrades the capability label (Certified to nothing) and is receipt-visible",
                            );
                            return None;
                        }
                        return Some(self.stmt(
                            start,
                            StmtKind::FieldDecl {
                                visibility: None,
                                name,
                                ty,
                                default,
                            },
                        ));
                    }
                    self.error_here("E-SYN-101", "expected a type after `:`");
                    return None;
                }
            }
        }

        // Bare field name (`x`): admission defaults `inputs:` entries to
        // Float64. The `Infer` marker is a parse-time placeholder so the
        // tree stays a `FieldDecl`; the formatter omits it.
        if matches!(
            self.peek_at(1),
            TokenKind::Newline | TokenKind::Dedent | TokenKind::Eof
        ) {
            self.advance();
            return Some(self.stmt(
                start,
                StmtKind::FieldDecl {
                    visibility: None,
                    name,
                    ty: TypeExpr {
                        kind: TypeKind::Path {
                            segments: vec!["Infer".into()],
                            generic_args: vec![],
                        },
                        source: start,
                    },
                    default: None,
                },
            ));
        }

        // assignments with indexed targets: `norm[b, t] = ...`: but
        // `minimize [a, b]` / `order [x, y]` are commands with a list
        // argument, so a `[` without `=` falls through to the command tail.
        if matches!(self.peek_at(1), TokenKind::LBracket) {
            let save = self.pos;
            self.advance(); // name
            if let Some(indices) = self.parse_index_list() {
                if self.eat(&TokenKind::Eq) {
                    self.skip_assignment_layout();
                    let value = self.parse_expr()?;
                    return Some(self.stmt(
                        start,
                        StmtKind::Assign {
                            target: Place {
                                segments: vec![name],
                                indices,
                                source: start.cover(self.last_span()),
                            },
                            value,
                        },
                    ));
                }
            }
            self.pos = save;
        }

        // Dotted section head: `core.policy:` (`lower declaration:`).
        // If it is not a section, fall through to the shared
        // segments handling (does not double-consume).
        if matches!(self.peek_at(1), TokenKind::Dot | TokenKind::PathSep) {
            let (segments, via_dot) = self.collect_segments_with_dots();
            if self.peek() == &TokenKind::Colon {
                self.advance();
                let suite = self.parse_suite()?;
                return Some(self.stmt(
                    start,
                    StmtKind::Section(Section {
                        name: segments.join("."),
                        generic: None,
                        args: None,
                        suite,
                        source: start.cover(self.last_span()),
                        head_source: start.cover(self.last_span()),
                    }),
                ));
            }
            if segments.is_empty() {
                self.error_here("E-SYN-110", "expected an expression");
                return None;
            }
            return self.finish_segments_statement(start, segments, via_dot);
        }

        // general: spaced idents, dotted places, commands
        let (segments, via_dot) = self.collect_segments_with_dots();
        if segments.is_empty() {
            self.error_here("E-SYN-110", "expected an expression");
            return None;
        }
        self.finish_segments_statement(start, segments, via_dot)
    }
}
