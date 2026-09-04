//! Literal primaries (numeric, string, booleans, parens, lists), table and
//! nabla literals, and string validation.

use super::*;

impl super::super::Parser {
    /// Table literal (U9): `|x y| 1, 2 | 3, 4 |`. Reached only when a pipe
    /// starts a primary (arm-leading and infix pipes never land here).
    /// Requires ≥2 header idents so `| cond => …` and `|x|`-shaped pipes
    /// fall through to the cases/or grammar unchanged.
    pub(super) fn parse_table_literal(&mut self) -> Option<Expr> {
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

    pub(super) fn parse_table_body(
        &mut self,
        start: emath_core::Span,
        headers: Vec<String>,
    ) -> Option<Expr> {
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

    /// Nabla-family call parse (pack). Targets and shapes mirror the
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
    pub(super) fn parse_nabla_call(&mut self, form: NablaForm) -> Option<Expr> {
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
    pub(super) fn parse_distribution_tag(&mut self) -> Result<Option<String>, ()> {
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
    /// U8: validate an interpolated
    /// string template at parse. Purity: a hole carries only a name or
    /// a dotted path — expressions, calls, and indexing refuse, so a
    /// side effect is impossible by grammar, not by discipline. The
    /// format spec is FIXED (`.` digits `f`); `{{`/`}}` escape literal
    /// braces; any other stray brace refuses. The template VALUE stays
    /// raw in the `Str` literal — substitution is the string world's
    /// job, which is outside the Phase 1 subset (every string value
    /// refuses at admission today); this validation is the grammar and
    /// its purity fence, the parse-level contract.
    pub(super) fn validate_interpolation(&mut self, value: &str, _start: Span) -> Option<()> {
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
    pub(super) fn validate_hole(&mut self, hole: &str) -> Option<()> {
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

    /// Literal-form primaries: numeric, string, `?`, booleans, parens, list
    /// literals. Split out of `parse_primary` for file size; arms are the
    /// original ones, byte-identical.
    pub(super) fn parse_primary_literal(&mut self, start: Span, depth: usize) -> Option<Expr> {
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
                // U8: the interpolation
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
            _ => unreachable!("parse_primary_literal routed a non-literal token"),
        }
    }
}
