//! Layout diagnostics.
//!
//! The layout rule (ch2): a NEWLINE token is emitted at a statement
//! boundary, and suppressed inside `()`, `[]`, and `{}` so multi-line
//! argument lists and parenthesized expressions lex as one flow. The lexer
//! owns suppression; this module owns the explanation and the typed
//! diagnostic for the failure mode where NEWLINE fires mid-expression.
//!
//! Bracket idiom: to continue an expression across lines, wrap it in `()`
//! (or `[]`). Inside brackets NEWLINE is suppressed, so the expression
//! parses as one flow. A bare hanging infix (`y = x +` then newline) is not
//! a continuation: `:` at the end of a binder line is not an incomplete
//! infix, and the body's first line is complete, so NEWLINE fires and the
//! parse splits (C4). Rewrite with brackets; the grammar does not consume a
//! continuation after a binder `:` today.

#![forbid(unsafe_code)]

use crate::token::TokenKind;

/// Typed diagnostic for a NEWLINE that fires where an expression was
/// expected to continue. Emitted when the token before the line break is a
/// binary operator: the expression is incomplete, and the fix is the bracket
/// idiom.
pub const E_SYN_HANGING_INFIX: &str = "E-SYN-153";

/// The operator tokens after which a NEWLINE means "hanging infix", not
/// "statement boundary". Ordered for the explanation message.
pub fn is_infix_operator(kind: &TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Plus
            | TokenKind::Minus
            | TokenKind::Star
            | TokenKind::Slash
            | TokenKind::SlashSlash
            | TokenKind::Caret
            | TokenKind::EqEq
            | TokenKind::NotEq
            | TokenKind::Imply
            | TokenKind::Iff
            | TokenKind::TildeTilde
            | TokenKind::PlusMinus
            | TokenKind::Le
            | TokenKind::Ge
            | TokenKind::Lt
            | TokenKind::Gt
            | TokenKind::Amp
            | TokenKind::Pipe
    )
}

/// Explanation of the layout state at a line break. Deterministic text; the
/// parser embeds it in E-SYN-153 so the diagnostic teaches the rule instead
/// of merely failing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutExplanation {
    /// NEWLINE fired at a statement boundary: expected.
    StatementBoundary,
    /// NEWLINE fired while the previous line ended in a binary operator:
    /// the expression is incomplete. The bracket idiom is the fix.
    HangingInfix,
}

impl LayoutExplanation {
    /// Deterministic help text for the diagnostic.
    #[must_use]
    pub fn help(self) -> &'static str {
        match self {
            LayoutExplanation::StatementBoundary => {
                "a line break ends a statement; the expression before it is complete"
            }
            LayoutExplanation::HangingInfix => {
                "the line ends with a binary operator, so the expression is incomplete; \
                 NEWLINE fires outside brackets, so the parse splits here. \
                 Bracket idiom: wrap the expression in `()` (or `[]`) to continue it \
                 across lines - NEWLINE is suppressed inside brackets"
            }
        }
    }

    /// Stable diagnostic code for the explanation.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            LayoutExplanation::StatementBoundary => "E-SYN-110",
            LayoutExplanation::HangingInfix => E_SYN_HANGING_INFIX,
        }
    }
}

/// Classify a line break seen while parsing: if the previous significant
/// token is a binary operator the break is a hanging infix (E-SYN-153 with
/// the bracket idiom help); otherwise it is an ordinary statement boundary.
#[must_use]
pub fn classify_line_break(previous_significant: Option<&TokenKind>) -> LayoutExplanation {
    match previous_significant {
        Some(kind) if is_infix_operator(kind) => LayoutExplanation::HangingInfix,
        _ => LayoutExplanation::StatementBoundary,
    }
}
