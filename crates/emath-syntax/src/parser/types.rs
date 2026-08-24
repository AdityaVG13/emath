use crate::token::{Keyword, TokenKind};
use crate::tree::{GenericArg, TypeExpr, TypeKind};

impl super::Parser {
    // ---- types ---------------------------------------------------------

    /// Parse a generic argument inside `<...>` or `[...]` at a type use site.
    /// C10: accepts types, value literals, expressions, and named args.
    pub(super) fn parse_generic_arg(&mut self) -> Option<GenericArg> {
        // Named argument: `identifier = expression`
        if let TokenKind::Ident(name) = self.peek().clone() {
            if matches!(self.peek_at(1), TokenKind::Eq) {
                self.advance(); // consume name
                self.advance(); // consume =
                let expr = self.parse_additive(0)?;
                return Some(GenericArg::Named {
                    name,
                    arg: Box::new(GenericArg::Value(expr)),
                });
            }
        }
        // Value argument: integer or float literal
        if matches!(self.peek(), TokenKind::Int(_) | TokenKind::Float(_)) {
            let expr = self.parse_additive(0)?;
            return Some(GenericArg::Value(expr));
        }
        // Bracket-list extent: `[N, N]` — parse as a list expression
        if matches!(self.peek(), TokenKind::LBracket) {
            let expr = self.parse_additive(0)?;
            return Some(GenericArg::Value(expr));
        }
        // Default: type expression
        let ty = self.parse_type_expr()?;
        Some(GenericArg::Type(ty))
    }

    pub(super) fn parse_type_expr(&mut self) -> Option<TypeExpr> {
        let start = self.current_span();
        let mut items = vec![self.parse_type_primary()?];
        while matches!(self.peek(), TokenKind::Star | TokenKind::Slash) {
            self.advance();
            items.push(self.parse_type_primary()?);
        }
        let base = if items.len() == 1 {
            let Some(item) = items.pop() else {
                return None;
            };
            item
        } else {
            TypeExpr {
                kind: TypeKind::Product(items),
                source: start.cover(self.last_span()),
            }
        };
        if self.eat_keyword(Keyword::In) {
            // U5: Domain annotation `Float64 in [lo, hi]` - when `in`
            // is followed by `[`, parse bounds as expressions.
            if matches!(self.peek(), TokenKind::LBracket) {
                self.advance(); // consume `[`
                let lo = self.parse_expr()?;
                if !self.eat(&TokenKind::Comma) {
                    self.error_here("E-SYN-102", "expected `,` in domain bounds");
                    return None;
                }
                let hi = self.parse_expr()?;
                if !self.eat(&TokenKind::RBracket) {
                    self.error_here("E-SYN-102", "expected `]` to close domain bounds");
                    return None;
                }
                return Some(TypeExpr {
                    kind: TypeKind::Domain {
                        base: Box::new(base),
                        lo: Box::new(lo),
                        hi: Box::new(hi),
                    },
                    source: start.cover(self.last_span()),
                });
            }
            // Unit annotation: `Float64 in m/s`
            let unit = self.parse_type_expr()?;
            return Some(TypeExpr {
                kind: TypeKind::In {
                    base: Box::new(base),
                    unit: Box::new(unit),
                },
                source: start.cover(self.last_span()),
            });
        }
        Some(base)
    }

    fn parse_type_primary(&mut self) -> Option<TypeExpr> {
        let start = self.current_span();
        match self.peek().clone() {
            TokenKind::Int(text) => {
                self.advance();
                Some(TypeExpr {
                    kind: TypeKind::Path {
                        segments: vec![text],
                        generic_args: Vec::new(),
                    },
                    source: start.cover(self.last_span()),
                })
            }
            TokenKind::Amp => {
                self.advance();
                let inner = self.parse_type_primary()?;
                Some(TypeExpr {
                    kind: TypeKind::Ref(Box::new(inner)),
                    source: start.cover(self.last_span()),
                })
            }
            TokenKind::LParen => {
                self.advance();
                let mut items = Vec::new();
                while !matches!(self.peek(), TokenKind::RParen | TokenKind::Eof) {
                    if self.eat(&TokenKind::Comma) {
                        continue;
                    }
                    items.push(self.parse_type_expr()?);
                }
                self.eat(&TokenKind::RParen);
                Some(TypeExpr {
                    kind: TypeKind::Tuple(items),
                    source: start.cover(self.last_span()),
                })
            }
            TokenKind::LBracket => {
                self.advance();
                let mut items = Vec::new();
                while !matches!(self.peek(), TokenKind::RBracket | TokenKind::Eof) {
                    if self.eat(&TokenKind::Comma) {
                        continue;
                    }
                    items.push(self.parse_type_expr()?);
                }
                self.eat(&TokenKind::RBracket);
                Some(TypeExpr {
                    kind: TypeKind::List(items),
                    source: start.cover(self.last_span()),
                })
            }
            TokenKind::Ident(_) | TokenKind::Keyword(Keyword::SelfKw | Keyword::Fn) => {
                let mut segments = Vec::new();
                match self.peek().clone() {
                    TokenKind::Ident(segment) => {
                        segments.push(segment);
                        self.advance();
                    }
                    TokenKind::Keyword(Keyword::SelfKw) => {
                        segments.push("Self".into());
                        self.advance();
                    }
                    _ => {
                        // `fn` type: function types are outside the Phase 1
                        // strict subset. Refuse loudly (E-TYPE-110) instead of
                        // recording a lossy Path(["fn"]) with the inner
                        // signature discarded.
                        self.error_here(
                            "E-TYPE-110",
                            "function types (fn(params) -> T) are outside the Phase 1 subset",
                        );
                        return None;
                    }
                }
                while matches!(self.peek(), TokenKind::PathSep) {
                    self.advance();
                    match self.peek().clone() {
                        TokenKind::Ident(segment) => {
                            segments.push(segment);
                            self.advance();
                        }
                        TokenKind::Star => {
                            self.advance();
                        }
                        _ => break,
                    }
                }
                let mut generic_args = Vec::new();
                if matches!(self.peek(), TokenKind::Lt) {
                    self.advance();
                    while !matches!(self.peek(), TokenKind::Gt | TokenKind::Eof) {
                        if self.eat(&TokenKind::Comma) {
                            continue;
                        }
                        if let Some(arg) = self.parse_generic_arg() {
                            generic_args.push(arg);
                        } else {
                            break;
                        }
                    }
                    if !self.eat(&TokenKind::Gt) {
                        self.error_here("E-SYN-102", "expected `>` to close type arguments");
                    }
                } else if matches!(self.peek(), TokenKind::LBracket) {
                    self.advance();
                    while !matches!(self.peek(), TokenKind::RBracket | TokenKind::Eof) {
                        if self.eat(&TokenKind::Comma) {
                            continue;
                        }
                        if let Some(arg) = self.parse_generic_arg() {
                            generic_args.push(arg);
                        } else {
                            break;
                        }
                    }
                    if !self.eat(&TokenKind::RBracket) {
                        self.error_here("E-SYN-102", "expected `]` to close type arguments");
                    }
                }
                Some(TypeExpr {
                    kind: TypeKind::Path {
                        segments,
                        generic_args,
                    },
                    source: start.cover(self.last_span()),
                })
            }
            other => {
                self.error_here(
                    "E-SYN-101",
                    format!("expected a type, found {}", other.describe()),
                );
                None
            }
        }
    }
}
