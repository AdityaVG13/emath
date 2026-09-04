//! Primary dispatch (`parse_primary`) over compound keyword forms.

use super::braket::BraketOperand;
use super::*;

impl super::super::Parser {
    pub(super) fn parse_primary(&mut self, depth: usize) -> Option<Expr> {
        let start = self.current_span();
        match self.peek().clone() {
            TokenKind::Int(_)
            | TokenKind::Float(_)
            | TokenKind::FloatUncertainty { .. }
            | TokenKind::Str(_)
            | TokenKind::Question
            | TokenKind::Keyword(Keyword::True)
            | TokenKind::Keyword(Keyword::False)
            | TokenKind::LParen
            | TokenKind::LBracket => self.parse_primary_literal(start, depth),
            TokenKind::Pipe => {
                // `|0⟩` — a pipe followed by an integer label and
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
                    // `|i⟩⟨j|` — the juxtaposed bra is the outer
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
                // Nabla pack: glyphs desugar to EXISTING
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
                // Jacobian sugar: `jacobian(<expr>) wrt
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
                // U6: `match subject { pattern
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
                    // B23: `graph { nodes; edges }`
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
                // F2: a NEWLINE after a binary
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
}
