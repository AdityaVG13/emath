//! layout tests migrated from the in-crate `#[cfg(test)]` module.

use emath_syntax::layout::*;
use emath_syntax::token::TokenKind;

#[test]
fn hanging_infix_classified_and_coded() {
    assert_eq!(
        classify_line_break(Some(&TokenKind::Plus)),
        LayoutExplanation::HangingInfix
    );
    assert_eq!(
        classify_line_break(Some(&TokenKind::Star)).code(),
        E_SYN_HANGING_INFIX
    );
}

#[test]
fn statement_boundary_classified() {
    assert_eq!(
        classify_line_break(Some(&TokenKind::Int("1".to_string()))),
        LayoutExplanation::StatementBoundary
    );
    assert_eq!(classify_line_break(None).code(), "E-SYN-110");
}

#[test]
fn every_infix_operator_maps_to_hanging_infix() {
    let operators = [
        TokenKind::Plus,
        TokenKind::Minus,
        TokenKind::Star,
        TokenKind::Slash,
        TokenKind::SlashSlash,
        TokenKind::Caret,
        TokenKind::EqEq,
        TokenKind::NotEq,
        TokenKind::Imply,
        TokenKind::Iff,
        TokenKind::TildeTilde,
        TokenKind::PlusMinus,
        TokenKind::Le,
        TokenKind::Ge,
        TokenKind::Lt,
        TokenKind::Gt,
        TokenKind::Amp,
        TokenKind::Pipe,
    ];
    for operator in &operators {
        assert_eq!(
            classify_line_break(Some(operator)),
            LayoutExplanation::HangingInfix,
            "{}",
            operator.describe()
        );
    }
}

#[test]
fn help_text_is_stable_and_names_the_idiom() {
    let help = LayoutExplanation::HangingInfix.help();
    assert!(help.contains("bracket"), "{help}");
    assert!(help.contains("NEWLINE"), "{help}");
}
