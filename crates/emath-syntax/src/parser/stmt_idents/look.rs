//! Lookahead and collection helpers.

use super::*;

impl super::super::Parser {
    /// For `<name>:` section heads: scan ahead for a matching `>` that is
    /// followed by `:` or `(` (a section head), not by another comparison
    /// operand (`a < b < c` is an expression).
    pub(super) fn lookahead_matches_lt_angle_head(&self) -> bool {
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
    pub(super) fn dotted_continues_expression(&self) -> bool {
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
    pub(super) fn looks_like_params_header(&mut self) -> bool {
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

    pub(super) fn parse_index_list(&mut self) -> Option<Vec<Expr>> {
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
    pub(super) fn collect_spaced_idents(&mut self) -> Vec<String> {
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

    pub(super) fn parse_arguments(&mut self) -> Option<Vec<Argument>> {
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
