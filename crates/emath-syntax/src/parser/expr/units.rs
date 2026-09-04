//! Unit-expression parsing: brackets, expressions, atoms.

use super::*;

impl super::super::Parser {
    /// Parse a compound-unit bracket `[unit m/s^2]` (F7/U4); the `unit`
    /// keyword disambiguates from indexing. `None` when not a unit bracket.
    pub(super) fn parse_unit_bracket(&mut self, depth: usize) -> Option<UnitExpr> {
        // Only enter if the next token is `[`.
        if !matches!(self.peek(), TokenKind::LBracket) {
            return None;
        }
        // Peek ahead: `[` must be followed by the identifier `unit`.
        let peek1 = self.peek_at(1).clone();
        if !matches!(peek1, TokenKind::Ident(name) if name == "unit") {
            return None;
        }
        // Consume `[` and `unit`.
        self.advance();
        self.advance();

        let unit_expr = self.parse_unit_expr(depth)?;

        if !self.eat(&TokenKind::RBracket) {
            self.error_here("E-SYN-102", "expected `]` to close unit bracket");
            return None;
        }
        Some(unit_expr)
    }

    /// Parse a unit expression: `m/s^2`, `kg*m^2/s^2`, `m/(s*s)`.
    /// Left-associative for `*` and `/` (C2 trap: `m/s*s` = length, not acceleration).
    pub(super) fn parse_unit_expr(&mut self, depth: usize) -> Option<UnitExpr> {
        let _ = depth;
        let mut left = self.parse_unit_atom()?;
        loop {
            match self.peek() {
                TokenKind::Star => {
                    self.advance();
                    let right = self.parse_unit_atom()?;
                    left = UnitExpr::Mul(Box::new(left), Box::new(right));
                }
                TokenKind::Slash => {
                    self.advance();
                    let right = self.parse_unit_atom()?;
                    left = UnitExpr::Div(Box::new(left), Box::new(right));
                }
                _ => break,
            }
        }
        Some(left)
    }

    /// Parse a unit atom: identifier, parenthesized group, or power.
    pub(super) fn parse_unit_atom(&mut self) -> Option<UnitExpr> {
        let base = match self.peek().clone() {
            TokenKind::Ident(name) => {
                self.advance();
                UnitExpr::Base(name)
            }
            TokenKind::LParen => {
                self.advance();
                let inner = self.parse_unit_expr(0)?;
                if !self.eat(&TokenKind::RParen) {
                    self.error_here("E-SYN-101", "expected `)` to close unit group");
                    return None;
                }
                inner
            }
            other => {
                self.error_here(
                    "E-SYN-101",
                    format!("expected unit name, found {}", other.describe()),
                );
                return None;
            }
        };
        // Check for power: `s^2`
        if matches!(self.peek(), TokenKind::Caret) {
            self.advance();
            let TokenKind::Int(exp_str) = self.peek().clone() else {
                self.error_here("E-SYN-101", "expected integer exponent after `^`");
                return None;
            };
            self.advance();
            let exp: i32 = exp_str.parse().unwrap_or(1);
            return Some(UnitExpr::Pow(Box::new(base), exp));
        }
        Some(base)
    }
}
