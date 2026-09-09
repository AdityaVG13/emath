//! layout tests migrated from the in-crate `#[cfg(test)]` module.

use emath_syntax::layout::*;
use emath_syntax::token::TokenKind;

use emath_test_harness::{Probe, boot};

#[test]
fn layout() {
    boot();
    let mut probe = Probe::new("layout tests migrated from the in-crate `#[cfg(test)]` module.");
    probe.case("hanging_infix_classified_and_coded", |p| {

    p.eq("1", classify_line_break(Some(&TokenKind::Plus)), LayoutExplanation::HangingInfix);
    p.eq("2", classify_line_break(Some(&TokenKind::Star)).code(), E_SYN_HANGING_INFIX);

    });
    probe.case("statement_boundary_classified", |p| {
    let f0 = p.failures().len();

    p.eq("1", classify_line_break(Some(&TokenKind::Int("1".to_string()))), LayoutExplanation::StatementBoundary);
    p.demand("2", (classify_line_break(None).code()) == ("E-SYN-110"), format!("expected {:?}, got {:?}", ("E-SYN-110"), (classify_line_break(None).code())));
    if p.failures().len() != f0 { return; }

    });
    probe.case("every_infix_operator_maps_to_hanging_infix", |p| {

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
        p.eq("1", classify_line_break(Some(operator)), LayoutExplanation::HangingInfix);
    }

    });
    probe.case("help_text_is_stable_and_names_the_idiom", |p| {
    let f0 = p.failures().len();

    let help = LayoutExplanation::HangingInfix.help();
    p.demand("1",help.contains("bracket"), format!( "{help}"));
    if p.failures().len() != f0 { return; }
    p.demand("2",help.contains("NEWLINE"), format!( "{help}"));
    if p.failures().len() != f0 { return; }

    });
    probe.finish();
}
