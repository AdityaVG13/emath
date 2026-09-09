//! The recursive-descent LaTeX math parser.

use super::*;

pub(super) struct Parser {
    pub(super) tokens: Vec<Token>,
    pub(super) index: usize,
    pub(super) end: usize,
}

impl Parser {
    pub(super) fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.index)
    }

    pub(super) fn bump(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.index).cloned()?;
        self.index += 1;
        Some(token)
    }

    pub(super) fn eat(&mut self, kind: &TokKind) -> bool {
        if self.peek().is_some_and(|token| token.kind == *kind) {
            self.index += 1;
            true
        } else {
            false
        }
    }

    pub(super) fn expect(&mut self, kind: TokKind) -> Result<Token, LayoutError> {
        match self.bump() {
            Some(token) if token.kind == kind => Ok(token),
            Some(token) => Err(LayoutError::UnexpectedToken {
                token: token_text(&token.kind),
                offset: token.start,
            }),
            None => Err(LayoutError::UnexpectedToken {
                token: "EOF".to_string(),
                offset: self.end,
            }),
        }
    }

    pub(super) fn parse_equality(&mut self) -> Result<Ast, LayoutError> {
        let mut left = self.parse_add()?;
        while self.eat(&TokKind::Eq) {
            let right = self.parse_add()?;
            let span = (left.span.0, right.span.1);
            left = Ast {
                kind: AstKind::Infix {
                    op: "=".to_string(),
                    left: Box::new(left),
                    right: Box::new(right),
                },
                span,
            };
        }
        Ok(left)
    }

    pub(super) fn parse_add(&mut self) -> Result<Ast, LayoutError> {
        let mut left = self.parse_mul()?;
        loop {
            let op = match self.peek().map(|token| &token.kind) {
                Some(TokKind::Plus) => "+",
                Some(TokKind::Minus) => "-",
                _ => break,
            };
            self.index += 1;
            let right = self.parse_mul()?;
            let span = (left.span.0, right.span.1);
            left = Ast {
                kind: AstKind::Infix {
                    op: op.to_string(),
                    left: Box::new(left),
                    right: Box::new(right),
                },
                span,
            };
        }
        Ok(left)
    }

    pub(super) fn parse_mul(&mut self) -> Result<Ast, LayoutError> {
        let mut left = self.parse_postfix()?;
        loop {
            let op = if self.eat(&TokKind::Star) {
                "*"
            } else if self.eat(&TokKind::Slash) {
                "/"
            } else if self.peek().is_some_and(|token| starts_atom(&token.kind)) {
                "*"
            } else {
                break;
            };
            let right = self.parse_postfix()?;
            let span = (left.span.0, right.span.1);
            left = Ast {
                kind: AstKind::Infix {
                    op: op.to_string(),
                    left: Box::new(left),
                    right: Box::new(right),
                },
                span,
            };
        }
        Ok(left)
    }

    pub(super) fn parse_postfix(&mut self) -> Result<Ast, LayoutError> {
        let mut atom = self.parse_atom()?;
        loop {
            if self.eat(&TokKind::Caret) {
                let exp = self.parse_script()?;
                let span = (atom.span.0, exp.span.1);
                atom = Ast {
                    kind: AstKind::Pow {
                        base: Box::new(atom),
                        exp: Box::new(exp),
                    },
                    span,
                };
            } else if self.eat(&TokKind::Underscore) {
                let sub = self.parse_script()?;
                let span = (atom.span.0, sub.span.1);
                atom = Ast {
                    kind: AstKind::Sub {
                        base: Box::new(atom),
                        sub: Box::new(sub),
                    },
                    span,
                };
            } else {
                break;
            }
        }
        Ok(atom)
    }

    pub(super) fn parse_script(&mut self) -> Result<Ast, LayoutError> {
        if self.eat(&TokKind::LBrace) {
            let inner = self.parse_equality()?;
            self.expect(TokKind::RBrace)?;
            return Ok(inner);
        }
        match self.peek().map(|token| &token.kind) {
            Some(TokKind::Letter(_) | TokKind::Number(_) | TokKind::Command(_)) => {
                self.parse_atom()
            }
            Some(kind) => Err(LayoutError::UnexpectedToken {
                token: token_text(kind),
                offset: self.peek().map_or(self.end, |token| token.start),
            }),
            None => Err(LayoutError::UnexpectedToken {
                token: "EOF".to_string(),
                offset: self.end,
            }),
        }
    }

    pub(super) fn parse_atom(&mut self) -> Result<Ast, LayoutError> {
        let token = self.bump().ok_or_else(|| LayoutError::UnexpectedToken {
            token: "EOF".to_string(),
            offset: self.end,
        })?;
        match token.kind {
            TokKind::Letter(ch) => Ok(Ast {
                kind: AstKind::Glyph(ch.to_string()),
                span: (token.start, token.end),
            }),
            TokKind::Number(text) => Ok(Ast {
                kind: AstKind::Glyph(text),
                span: (token.start, token.end),
            }),
            TokKind::LParen => {
                let inner = self.parse_equality()?;
                self.expect(TokKind::RParen)?;
                Ok(inner)
            }
            TokKind::LBrace => {
                let inner = self.parse_equality()?;
                self.expect(TokKind::RBrace)?;
                Ok(inner)
            }
            TokKind::Command(name) => self.parse_command(name, token.start, token.end),
            other => Err(LayoutError::UnexpectedToken {
                token: token_text(&other),
                offset: token.start,
            }),
        }
    }

    pub(super) fn parse_command(
        &mut self,
        name: String,
        start: usize,
        end: usize,
    ) -> Result<Ast, LayoutError> {
        if GREEK.contains(&name.as_str()) || name == "to" {
            return Ok(Ast {
                kind: AstKind::Glyph(name),
                span: (start, end),
            });
        }
        match name.as_str() {
            "frac" => {
                let num = self.parse_braced()?;
                let den = self.parse_braced()?;
                Ok(Ast {
                    span: (start, den.span.1),
                    kind: AstKind::Frac {
                        num: Box::new(num),
                        den: Box::new(den),
                    },
                })
            }
            "sqrt" => {
                let inner = self.parse_braced()?;
                Ok(Ast {
                    span: (start, inner.span.1),
                    kind: AstKind::Sqrt(Box::new(inner)),
                })
            }
            "sum" | "prod" => self.parse_sum_like(name, start),
            "int" => self.parse_integral(start),
            "lim" => self.parse_limit(start),
            _ => Err(LayoutError::UnknownMacro {
                name,
                offset: start,
            }),
        }
    }

    pub(super) fn parse_braced(&mut self) -> Result<Ast, LayoutError> {
        self.expect(TokKind::LBrace)?;
        let inner = self.parse_equality()?;
        self.expect(TokKind::RBrace)?;
        Ok(inner)
    }

    pub(super) fn parse_sum_like(
        &mut self,
        name: String,
        start: usize,
    ) -> Result<Ast, LayoutError> {
        self.expect(TokKind::Underscore)?;
        self.expect(TokKind::LBrace)?;
        let bound = self.expect_ident()?;
        self.expect(TokKind::Eq)?;
        let lower = self.parse_equality()?;
        self.expect(TokKind::RBrace)?;
        self.expect(TokKind::Caret)?;
        let upper = self.parse_script()?;
        let body = self.parse_optional_body()?;
        let end = body.as_ref().map_or(upper.span.1, |body| body.span.1);
        Ok(Ast {
            span: (start, end),
            kind: AstKind::BigOp {
                name,
                bound: Some(bound),
                lower: Some(Box::new(lower)),
                upper: Some(Box::new(upper)),
                body: body.map(Box::new),
            },
        })
    }

    pub(super) fn parse_integral(&mut self, start: usize) -> Result<Ast, LayoutError> {
        self.expect(TokKind::Underscore)?;
        let lower = self.parse_script()?;
        self.expect(TokKind::Caret)?;
        let upper = self.parse_script()?;
        let body = self.parse_optional_body()?;
        let end = body.as_ref().map_or(upper.span.1, |body| body.span.1);
        Ok(Ast {
            span: (start, end),
            kind: AstKind::BigOp {
                name: "int".to_string(),
                bound: None,
                lower: Some(Box::new(lower)),
                upper: Some(Box::new(upper)),
                body: body.map(Box::new),
            },
        })
    }

    pub(super) fn parse_limit(&mut self, start: usize) -> Result<Ast, LayoutError> {
        self.expect(TokKind::Underscore)?;
        self.expect(TokKind::LBrace)?;
        let bound = self.expect_ident()?;
        match self.bump() {
            Some(token) if matches!(token.kind, TokKind::Command(ref name) if name == "to") => {}
            Some(token) => {
                return Err(LayoutError::UnexpectedToken {
                    token: token_text(&token.kind),
                    offset: token.start,
                });
            }
            None => {
                return Err(LayoutError::UnexpectedToken {
                    token: "EOF".to_string(),
                    offset: self.end,
                });
            }
        }
        let to = self.parse_equality()?;
        self.expect(TokKind::RBrace)?;
        let body = self.parse_optional_body()?;
        let end = body.as_ref().map_or(to.span.1, |body| body.span.1);
        Ok(Ast {
            span: (start, end),
            kind: AstKind::BigOp {
                name: "lim".to_string(),
                bound: Some(bound),
                lower: Some(Box::new(to)),
                upper: None,
                body: body.map(Box::new),
            },
        })
    }

    pub(super) fn expect_ident(&mut self) -> Result<String, LayoutError> {
        match self.bump() {
            Some(Token {
                kind: TokKind::Letter(ch),
                ..
            }) => Ok(ch.to_string()),
            Some(Token {
                kind: TokKind::Command(name),
                ..
            }) if GREEK.contains(&name.as_str()) => Ok(name),
            Some(token) => Err(LayoutError::UnexpectedToken {
                token: token_text(&token.kind),
                offset: token.start,
            }),
            None => Err(LayoutError::UnexpectedToken {
                token: "EOF".to_string(),
                offset: self.end,
            }),
        }
    }

    pub(super) fn parse_optional_body(&mut self) -> Result<Option<Ast>, LayoutError> {
        if self.peek().is_some_and(|token| starts_atom(&token.kind)) {
            Ok(Some(self.parse_postfix()?))
        } else {
            Ok(None)
        }
    }
}
