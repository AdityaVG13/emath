//! Equation-tail parsing.

use super::*;

impl super::super::Parser {
    /// After an expression, accept an equation tail: `= rhs` on the same
    /// line or on an indented continuation line (`mass * derivative(v)\n
    ///     = -(...)`). Returns `None` for a plain expression statement.
    pub(in crate::parser) fn parse_equation_tail(
        &mut self,
        left: &Expr,
        start: Span,
    ) -> Option<Stmt> {
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
}
