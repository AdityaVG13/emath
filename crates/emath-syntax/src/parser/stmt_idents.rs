use crate::token::{Keyword, TokenKind};
use crate::tree::{
    Argument, ArgumentValue, CommandArgument, Expr, ExprKind, Place, Section, SeriesExtrapolation,
    SeriesInterpolation, Stmt, StmtKind, TypeExpr, TypeKind,
};
use emath_core::Span;

impl super::Parser {
    // ---- ident-headed statements --------------------------------------

    pub(super) fn parse_ident_statement(&mut self, start: Span, name: String) -> Option<Stmt> {
        match name.as_str() {
            // Unit queries in statement position (`unit of x == m` inside
            // `constraints:`): `unit`/`dimension` activate only before `of`
            // (contextual keywords), so an ordinary identifier or field
            // named `unit` is untouched. Falls through to the shared
            // expression-statement shape (with equation tail).
            "unit" | "dimension" if matches!(self.peek_at(1), TokenKind::Ident(id) if id == "of") =>
            {
                let expr = self.parse_expr()?;
                if let Some(stmt) = self.parse_equation_tail(&expr, start) {
                    return Some(stmt);
                }
                Some(self.stmt(start, StmtKind::Expr(expr)))
            }
            // Constructor-level heads (spec 09 / `emath-nko`):
            // `world constructor <name>:` / `artifact constructor <name>:`
            "world" | "artifact"
                if matches!(self.peek_at(1), TokenKind::Ident(second) if second == "constructor")
                    && matches!(self.peek_at(2), TokenKind::Ident(_))
                    && matches!(self.peek_at(3), TokenKind::Colon) =>
            {
                self.advance(); // level word
                self.advance(); // `constructor`
                let TokenKind::Ident(generic) = self.peek().clone() else {
                    return None;
                };
                self.advance();
                self.eat(&TokenKind::Colon);
                let suite = self.parse_section_suite(&name)?;
                Some(self.stmt(
                    start,
                    StmtKind::Section(Section {
                        name,
                        generic: Some(generic),
                        args: None,
                        suite,
                        source: start.cover(self.last_span()),
                        head_source: start.cover(self.last_span()),
                    }),
                ))
            }
            "record" | "variant" | "trait" | "implementation" | "predicate" => {
                self.advance();
                let TokenKind::Ident(decl_name) = self.peek().clone() else {
                    self.error_here("E-SYN-110", "expected a name after `{name}`");
                    return None;
                };
                self.advance();
                let mut generic = Some(decl_name);
                // `predicate <candidate>(w: Witness):` form
                if matches!(self.peek(), TokenKind::Lt) && name != "implementation" {
                    self.advance();
                    if let TokenKind::Ident(inner) = self.peek().clone() {
                        generic = Some(inner);
                        self.advance();
                    }
                    self.eat(&TokenKind::Gt);
                }
                let args = if matches!(self.peek(), TokenKind::LParen) {
                    // Broken argument lists must not be recorded as section
                    // heads with `args: None` (silent drop); refuse the
                    // whole statement instead.
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
                    self.error_here("E-SYN-111", "expected `:` after declaration name");
                    return None;
                }
                let suite = self.parse_suite()?;
                Some(self.stmt(
                    start,
                    StmtKind::Section(Section {
                        name,
                        generic,
                        args,
                        suite,
                        source: start.cover(self.last_span()),
                        head_source: start.cover(self.last_span()),
                    }),
                ))
            }
            "type" => {
                self.advance();
                let TokenKind::Ident(alias) = self.peek().clone() else {
                    self.error_here("E-SYN-110", "expected a type name after `type`");
                    return None;
                };
                self.advance();
                if self.eat(&TokenKind::Eq) {
                    // `type Alias = RHS`: Phase 1 defines no alias semantics,
                    // and the tree would record only Command["type", alias]
                    // with `argument: None`, silently dropping the RHS.
                    // Refuse loudly (E-TYPE-111) per the no-silent-accept rule.
                    self.error_here(
                        "E-TYPE-111",
                        "type aliases (type X = T) are outside the Phase 1 subset",
                    );
                    None
                } else if self.eat(&TokenKind::Colon) {
                    let suite = self.parse_suite()?;
                    Some(self.stmt(
                        start,
                        StmtKind::Section(Section {
                            name: "type".to_string(),
                            generic: Some(alias),
                            args: None,
                            suite,
                            source: start.cover(self.last_span()),
                            head_source: start.cover(self.last_span()),
                        }),
                    ))
                } else {
                    self.error_here("E-SYN-101", "expected `=` or `:` after type name");
                    None
                }
            }
            "given" => {
                self.advance();
                let TokenKind::Ident(given_name) = self.peek().clone() else {
                    self.error_here("E-SYN-110", "expected a name after `given`");
                    return None;
                };
                self.advance();
                if !self.eat(&TokenKind::Eq) {
                    self.error_here("E-SYN-111", "expected `=` in `given` binding");
                    return None;
                }
                self.skip_assignment_layout();
                let value = self.parse_expr()?;
                Some(self.stmt(
                    start,
                    StmtKind::Given {
                        name: given_name,
                        value,
                    },
                ))
            }
            // 04 §5.3 (emath-r3-fit-goal-4xjh): the generic fit-goal
            // grammar `fit <params> to <observable>:` — parameters and
            // observable are plain vocabulary data (NO domain model in
            // the parser); the honesty contract (Fitted provenance,
            // explicit residual weighting, AuthorityEscalation on
            // unidentifiable tight CI) lives in admission/lowering and
            // the generic fit-goal runtime. Previously the row refused
            // E-SYN-101 naming the design follow-up.
            "fit" if matches!(self.peek_at(1), TokenKind::Ident(_)) => {
                self.advance(); // `fit`
                let mut args = Vec::new();
                loop {
                    let TokenKind::Ident(parameter) = self.peek().clone() else {
                        break;
                    };
                    let span = self.current_span();
                    self.advance();
                    args.push(Argument {
                        name: None,
                        value: ArgumentValue::Expr(Expr {
                            kind: ExprKind::Path {
                                segments: vec![parameter],
                                generics: None,
                            },
                            source: span,
                        }),
                        source: span,
                    });
                    if !self.eat(&TokenKind::Comma) {
                        break;
                    }
                }
                if args.is_empty() {
                    self.error_here(
                        "E-SYN-101",
                        "`fit` requires at least one parameter: `fit <params> to <observable>:`",
                    );
                    return None;
                }
                let TokenKind::Ident(word) = self.peek().clone() else {
                    self.error_here(
                        "E-SYN-101",
                        "expected `to` in fit goal (`fit <params> to <observable>:`)",
                    );
                    return None;
                };
                if word != "to" {
                    self.error_here(
                        "E-SYN-101",
                        format!(
                            "expected `to` after the fit parameters, found `{word}` \
                             (`fit <params> to <observable>:`)"
                        ),
                    );
                    return None;
                }
                self.advance(); // `to`
                let TokenKind::Ident(observable) = self.peek().clone() else {
                    self.error_here(
                        "E-SYN-101",
                        "expected an observable name after `to` (`fit <params> to <observable>:`)",
                    );
                    return None;
                };
                self.advance();
                if !self.eat(&TokenKind::Colon) {
                    self.error_here("E-SYN-111", "expected `:` after the fit goal head");
                    return None;
                }
                let suite = self.parse_suite()?;
                Some(self.stmt(
                    start,
                    StmtKind::Section(Section {
                        name: "fit".to_string(),
                        generic: Some(observable),
                        args: Some(args),
                        suite,
                        source: start.cover(self.last_span()),
                        head_source: start.cover(self.last_span()),
                    }),
                ))
            }
            // 04 §2.5 (emath-r3-lagrangian-action-nf7s): the action
            // integral binder `S = action integral t in t0..t1:
            // L(q(t), der(q(t)), t)`. The action is a FUNCTIONAL (a
            // map from trajectories to Real) — it admits only
            // variation/evaluation goals, never scalar composition.
            // The binder grammar is a design follow-up; refuse at the
            // grammar naming it (previously the row died as `unknown
            // variable action` plus a generic row-shape error). A bare
            // `action` stays a plain identifier — only the two-word
            // spelling fences.
            "action" if matches!(self.peek_at(1), TokenKind::Keyword(Keyword::Integral)) => {
                self.advance(); // `action`
                self.advance(); // `integral`
                while !matches!(
                    self.peek(),
                    TokenKind::Newline | TokenKind::Dedent | TokenKind::Eof
                ) {
                    self.advance();
                }
                self.error_here(
                    "E-SYN-101",
                    "`action integral t in t0..t1: L(...)` is outside the Phase 1 subset — the action/variation design follow-up (emath-r3-lagrangian-action-nf7s) must first settle the design of record: the action is a Functional (admits only variation/evaluation goals, never scalar composition), the `variation <S> wrt q:` goal lowers to core goals via the Euler-Lagrange operator built from the admitted partial/total derivatives, and the boundary condition (`fixed_endpoints`) is part of the goal identity hash; evidence must use admitted surface `derivative(derivative(q) wrt t) wrt t`, never `d²q/dt²` (C14)",
                );
                None
            }
            // 04 §2.5 (emath-r3-lagrangian-action-nf7s): the variation
            // goal `variation <S> wrt q:` — a custom goal verb that
            // lowers to core goals (ch9 contract): the Euler-Lagrange
            // residual built from admitted derivatives, simplified by
            // the native symbolic engine, solved as declared. The goal
            // grammar is a design follow-up; refuse at the grammar
            // naming it (previously the row died with `unexpected ':'`
            // + `unexpected indent`). A bare `variation` stays a plain
            // identifier.
            "variation" if matches!(self.peek_at(1), TokenKind::Ident(_)) => {
                self.advance(); // `variation`
                while !matches!(
                    self.peek(),
                    TokenKind::Newline | TokenKind::Dedent | TokenKind::Eof
                ) {
                    self.advance();
                }
                self.error_here(
                    "E-SYN-101",
                    "`variation <S> wrt q:` is outside the Phase 1 subset — the variation-goal design follow-up (emath-r3-lagrangian-action-nf7s) lowers it to core goals: the Euler-Lagrange residual ∂L/∂q − d/dt(∂L/∂q̇) built from the admitted partial/total derivative operators, simplified by the native symbolic engine and solved as declared; `yield euler_lagrange` names the output form and `boundary: fixed_endpoints` is part of the goal identity (changes the answer, changes the hash); a versioned provider is the ch9 alternative, core lowering is the design of record",
                );
                None
            }
            "expect" => {
                self.advance();
                self.skip_assignment_layout();
                let expr = self.parse_expr()?;
                Some(self.stmt(start, StmtKind::Expect(expr)))
            }
            // 04 §5.2 (emath-r3-observations-9ffu): `obs <name>[: <type>]
            // = <data>` inside an `observations:` section — a measured
            // datum, never a definition. The row is losslessly a
            // `FieldDecl` with its value as the default; admission owns
            // the read-only semantics (E-OBS-WRITE). The `obs` prefix is
            // section-implied, so the formatter restores it on output.
            "obs" => {
                self.advance();
                let TokenKind::Ident(obs_name) = self.peek().clone() else {
                    self.error_here("E-SYN-110", "expected a name after `obs`");
                    return None;
                };
                self.advance();
                let annotation = if self.eat(&TokenKind::Colon) {
                    let Some(ty) = self.parse_type_expr() else {
                        self.error_here("E-SYN-110", "expected a type after `obs <name>:`");
                        return None;
                    };
                    Some(ty)
                } else {
                    None
                };
                if !self.eat(&TokenKind::Eq) {
                    self.error_here(
                        "E-SYN-111",
                        "expected `=` in `obs` row (obs <name>[: type] = data)",
                    );
                    return None;
                }
                self.skip_assignment_layout();
                let Some(value) = self.parse_expr() else {
                    self.error_here("E-SYN-101", "expected a data value after `=`");
                    return None;
                };
                let ty = annotation.unwrap_or(TypeExpr {
                    kind: TypeKind::Path {
                        segments: vec!["Infer".into()],
                        generic_args: vec![],
                    },
                    source: start,
                });
                Some(self.stmt(
                    start,
                    StmtKind::FieldDecl {
                        visibility: None,
                        name: obs_name,
                        ty,
                        default: Some(value),
                    },
                ))
            }
            "extend" => {
                // `extends model` style: verify corpus usage once
                let segments = self.collect_spaced_idents();
                Some(self.stmt(
                    start,
                    StmtKind::Command {
                        head: segments,
                        argument: None,
                    },
                ))
            }
            // B04/B06: contextual keywords `limit`, `sample_limit`, `series`
            // in statement position. When followed by the right tokens,
            // parse as an expression statement, not a command.
            "limit" | "sample_limit"
                if matches!(self.peek_at(1), TokenKind::Ident(_))
                    && matches!(self.peek_at(2), TokenKind::Arrow) =>
            {
                let expr = self.parse_expr()?;
                Some(self.stmt(start, StmtKind::Expr(expr)))
            }
            "series"
                if matches!(self.peek_at(1), TokenKind::Ident(_))
                    && matches!(self.peek_at(2), TokenKind::Keyword(Keyword::In)) =>
            {
                let expr = self.parse_expr()?;
                Some(self.stmt(start, StmtKind::Expr(expr)))
            }
            // U1: `cases [subject]: | ...` in statement position.
            "cases"
                if matches!(self.peek_at(1), TokenKind::Colon)
                    || (matches!(self.peek_at(1), TokenKind::Ident(_))
                        && matches!(self.peek_at(2), TokenKind::Colon)) =>
            {
                let expr = self.parse_expr()?;
                Some(self.stmt(start, StmtKind::Expr(expr)))
            }
            _ => self.parse_default_ident_statement(start),
        }
    }

    /// Generic ident-headed statement: sections, fields, commands, assigns.
    fn parse_default_ident_statement(&mut self, start: Span) -> Option<Stmt> {
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
                        // 05 §7.1 (emath-r3-refinement-types-rrhd): the
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
                                "`name: Type where <predicate>` refinement rows are outside the Phase 1 subset — the refinement-types design follow-up (emath-r3-refinement-types-rrhd) must first settle: predicates stay in a total, decidable fragment (same discipline as ch8's bounded lowering language; type checking deterministic and budgeted), refinements are recorded in semantic identity (as domain annotations already are), and conflicting constraints are NAMED in diagnostics (ch16 gate 6) so refinements stay usable rather than feared; the admitted refinement surface today is the domain annotation `Type in [lo, hi]`; there is no trust-me cast — the runtime-checked cast downgrades the capability label (Certified to nothing) and is receipt-visible",
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

    /// 04 §5.4 (emath-r3-timeseries-1nsa): optional series-policy suffix
    /// on a definition/observation value — `with interpolation: <mode>
    /// [, extrapolation: <mode>]`. The policy is part of the value's
    /// identity; an absent part means the language default (`refuse` for
    /// extrapolation, declared-required interpolation handled by
    /// admission). Only valid in row-value position, never mid-expression.
    fn parse_series_policy_suffix(&mut self, value: Expr, start: Span) -> Option<Expr> {
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
    fn finish_segments_statement(
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
            // 04 §2.5 (emath-r3-lagrangian-action-nf7s): the action
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
                    "`action integral t in t0..t1: L(...)` is outside the Phase 1 subset — the action/variation design follow-up (emath-r3-lagrangian-action-nf7s) must first settle the design of record: the action is a Functional (admits only variation/evaluation goals, never scalar composition), the `variation <S> wrt q:` goal lowers to core goals via the Euler-Lagrange operator built from the admitted partial/total derivatives, and the boundary condition (`fixed_endpoints`) is part of the goal identity hash; evidence must use admitted surface `derivative(derivative(q) wrt t) wrt t`, never `d²q/dt²` (C14)",
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
                "definition rows do not carry units: `name in unit = value` is outside the Phase 1 subset; put the unit on the quantity (`0.30 [unit 1/day]`) or on the declared input/output row (`k: Float64 in 1/day`); unit-carrying measured rate parameters are the bio dynamics field-pack follow-up (emath-r3-bio-dynamics-ephb)",
            );
            return None;
        }
        let (head, argument) = self.parse_command_tail(segments)?;
        Some(self.stmt(start, StmtKind::Command { head, argument }))
    }

    /// After an expression, accept an equation tail: `= rhs` on the same
    /// line or on an indented continuation line (`mass * derivative(v)\n
    ///     = -(...)`). Returns `None` for a plain expression statement.
    pub(super) fn parse_equation_tail(&mut self, left: &Expr, start: Span) -> Option<Stmt> {
        if self.peek() == &TokenKind::Eq {
            self.advance();
            let opened = self.skip_assignment_layout();
            let right = self.parse_expr()?;
            self.close_assignment_indent(opened);
            return Some(self.stmt(
                start,
                StmtKind::Equation {
                    left: left.clone(),
                    right,
                },
            ));
        }
        if self.peek() == &TokenKind::Newline {
            let save = self.pos;
            self.skip_newlines();
            let indented = matches!(self.peek(), TokenKind::Indent);
            if indented {
                self.advance();
            }
            // Same-level or deeper continuation: `mass * derivative(v)`
            // newline `= -(...)`. No statement begins with `=`, so a
            // leading `=` is unambiguously an equation continuation.
            if self.peek() == &TokenKind::Eq {
                self.advance();
                self.skip_assignment_layout();
                let right = self.parse_expr()?;
                if indented {
                    // The continuation line added a temporary indent;
                    // balance it with exactly one Dedent after the line
                    // (the suite's own closing Dedent stays for the
                    // enclosing block).
                    if self.peek() == &TokenKind::Newline {
                        self.skip_newlines();
                    }
                    if matches!(self.peek(), TokenKind::Dedent) {
                        self.advance();
                    }
                }
                return Some(self.stmt(
                    start,
                    StmtKind::Equation {
                        left: left.clone(),
                        right,
                    },
                ));
            }
            self.pos = save;
        }
        None
    }

    /// For `<name>:` section heads: scan ahead for a matching `>` that is
    /// followed by `:` or `(` (a section head), not by another comparison
    /// operand (`a < b < c` is an expression).
    fn lookahead_matches_lt_angle_head(&self) -> bool {
        let max = self.tokens.len().saturating_sub(1);
        let mut depth = 0_u32;
        let mut index = self.pos + 1;
        while index < max {
            match &self.tokens[index].kind {
                TokenKind::Lt => depth += 1,
                TokenKind::Gt => {
                    if depth == 0 {
                        return false;
                    }
                    depth -= 1;
                    if depth == 0 {
                        return matches!(
                            self.tokens.get(index + 1).map(|t| &t.kind),
                            Some(TokenKind::Colon | TokenKind::LParen)
                        );
                    }
                }
                TokenKind::Newline | TokenKind::Eof => return false,
                _ => {}
            }
            index += 1;
        }
        false
    }

    /// For dotted/`::`-led statements, decide whether the remainder is an
    /// expression (`candidate.reuse_probability ^ alpha`), a section head
    /// (`core.policy:`), or a command / assignment.
    fn dotted_continues_expression(&self) -> bool {
        let max = self.tokens.len().saturating_sub(1);
        let mut index = self.pos + 1;
        while index < max {
            match &self.tokens[index].kind {
                TokenKind::Dot | TokenKind::PathSep | TokenKind::Ident(_) => {}
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
                | TokenKind::Eq => return true,
                _ => return false,
            }
            index += 1;
        }
        false
    }

    /// Scan a candidate `name (...)` to see whether the parens contain a
    /// parameter list (`ident : type`) or `->` follows the closing paren.
    fn looks_like_params_header(&mut self) -> bool {
        // A `name(arg-list)` is a function declaration when the parens hold
        // a typed parameter (`ident : Type`), or when `->` follows the
        // closing paren (`define score(candidate) -> Real:`). A bare call
        // (`solve minimize(dot(...))`) is an expression.
        let mut depth: u32 = 0;
        let mut index = 1;
        let mut saw_typed = false;
        let max = self.tokens.len().saturating_sub(1);
        while index < max {
            let absolute = (self.pos + index).min(max);
            match &self.tokens[absolute].kind {
                TokenKind::LParen | TokenKind::LBracket => depth += 1,
                TokenKind::RParen | TokenKind::RBracket => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        let after = self.tokens.get(absolute + 1).map(|t| &t.kind);
                        if saw_typed {
                            return matches!(after, Some(TokenKind::Arrow | TokenKind::Colon))
                                || after == Some(&TokenKind::Newline);
                        }
                        return matches!(after, Some(TokenKind::Arrow));
                    }
                }
                TokenKind::Colon => saw_typed = true,
                TokenKind::Newline | TokenKind::Eof if depth == 0 => return false,
                _ => {}
            }
            index += 1;
        }
        false
    }

    fn parse_index_list(&mut self) -> Option<Vec<Expr>> {
        if !self.eat(&TokenKind::LBracket) {
            return None;
        }
        let mut indices = Vec::new();
        while !matches!(self.peek(), TokenKind::RBracket | TokenKind::Eof) {
            if self.eat(&TokenKind::Comma) {
                continue;
            }
            indices.push(self.parse_expr()?);
        }
        if !self.eat(&TokenKind::RBracket) {
            self.error_here("E-SYN-102", "expected `]` to close indices");
        }
        Some(indices)
    }

    /// Collect space-separated identifiers (`extends model`, `budget iterations`).
    fn collect_spaced_idents(&mut self) -> Vec<String> {
        let mut segments = Vec::new();
        while let TokenKind::Ident(next) = self.peek().clone() {
            segments.push(next);
            self.advance();
        }
        segments
    }

    /// Collect identifiers optionally joined by `.` or `::`; an identifier
    /// opening generics, a comparison, or a call is left for the command tail.
    pub(super) fn collect_segments_with_dots(&mut self) -> (Vec<String>, bool) {
        let mut segments = Vec::new();
        let mut via_dot = false;
        if let TokenKind::Ident(first) = self.peek().clone() {
            segments.push(first);
            self.advance();
        }
        loop {
            if let TokenKind::Ident(next) = self.peek().clone() {
                // Stop before operators, comparisons, generics, calls, or a
                // following `<`: those start an argument expression
                // (`when x >= 0`, `error max_absolute <= 2e-8`,
                // `Tensor<Float32, [D]>`).
                if matches!(
                    self.peek_at(1),
                    TokenKind::Lt
                        | TokenKind::Le
                        | TokenKind::Ge
                        | TokenKind::Gt
                        | TokenKind::EqEq
                        | TokenKind::NotEq
                        | TokenKind::Plus
                        | TokenKind::Minus
                        | TokenKind::Star
                        | TokenKind::Slash
                        | TokenKind::Caret
                        | TokenKind::LParen
                        | TokenKind::LBracket
                        | TokenKind::Eq
                ) {
                    break;
                }
                segments.push(next);
                self.advance();
                continue;
            }
            if matches!(self.peek(), TokenKind::Dot | TokenKind::PathSep)
                && matches!(self.peek_at(1), TokenKind::Ident(_))
            {
                // SURF-0013: keep the joined path as ONE segment so the
                // spelling survives the tree (`produce rust.library` keeps
                // its dot; command heads render back byte-identically).
                let separator = if matches!(self.peek(), TokenKind::PathSep) {
                    "::"
                } else {
                    "."
                };
                self.advance();
                if let TokenKind::Ident(next) = self.peek().clone() {
                    let last = segments.pop().unwrap_or_default();
                    segments.push(format!("{last}{separator}{next}"));
                    via_dot = true;
                    self.advance();
                    continue;
                }
            }
            break;
        }
        (segments, via_dot)
    }

    fn parse_arguments(&mut self) -> Option<Vec<Argument>> {
        if !self.eat(&TokenKind::LParen) {
            return None;
        }
        let mut args = Vec::new();
        while !matches!(self.peek(), TokenKind::RParen | TokenKind::Eof) {
            if self.eat(&TokenKind::Comma) {
                continue;
            }
            let start = self.current_span();
            if let TokenKind::Ident(name) = self.peek().clone() {
                if matches!(self.peek_at(1), TokenKind::Colon) {
                    self.advance();
                    self.advance();
                    if let Some(ty) = self.parse_type_expr() {
                        args.push(Argument {
                            name: Some(name),
                            value: ArgumentValue::Type(ty),
                            source: start.cover(self.last_span()),
                        });
                        continue;
                    }
                    return None;
                }
            }
            let expr = self.parse_expr()?;
            args.push(Argument {
                name: None,
                value: ArgumentValue::Expr(expr),
                source: start.cover(self.last_span()),
            });
        }
        self.eat(&TokenKind::RParen);
        Some(args)
    }
}
