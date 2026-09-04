use crate::token::{Keyword, TokenKind};
use crate::tree::{
    Argument, ArgumentValue, CommandArgument, Expr, ExprKind, Place, Section, SeriesExtrapolation,
    SeriesInterpolation, Stmt, StmtKind, TypeExpr, TypeKind,
};
use emath_core::Span;

mod default;
mod eq;
mod look;
mod series;

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
            // 04 §5.3: the generic fit-goal
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
            // 04 §2.5: the action
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
                    "`action integral t in t0..t1: L(...)` is outside the Phase 1 subset — the action/variation design follow-up must first settle the design of record: the action is a Functional (admits only variation/evaluation goals, never scalar composition), the `variation <S> wrt q:` goal lowers to core goals via the Euler-Lagrange operator built from the admitted partial/total derivatives, and the boundary condition (`fixed_endpoints`) is part of the goal identity hash; evidence must use admitted surface `derivative(derivative(q) wrt t) wrt t`, never `d²q/dt²` (C14)",
                );
                None
            }
            // 04 §2.5: the variation
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
                    "`variation <S> wrt q:` is outside the Phase 1 subset — the variation-goal design follow-up lowers it to core goals: the Euler-Lagrange residual ∂L/∂q − d/dt(∂L/∂q̇) built from the admitted partial/total derivative operators, simplified by the native symbolic engine and solved as declared; `yield euler_lagrange` names the output form and `boundary: fixed_endpoints` is part of the goal identity (changes the answer, changes the hash); a versioned provider is the ch9 alternative, core lowering is the design of record",
                );
                None
            }
            "expect" => {
                self.advance();
                self.skip_assignment_layout();
                let expr = self.parse_expr()?;
                Some(self.stmt(start, StmtKind::Expect(expr)))
            }
            // 04 §5.2: `obs <name>[: <type>]
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
}
