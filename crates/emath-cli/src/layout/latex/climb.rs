//! Sequence climbing: precedence climbing over lowered items.

use super::*;

pub(super) enum SeqItem {
    Term(BinderTerm),
    Op(String),
}

pub(super) fn climb_eq(
    items: &[SeqItem],
    start: usize,
) -> Result<(BinderTerm, usize), LayoutError> {
    let (mut left, mut index) = climb_add(items, start)?;
    while matches!(items.get(index), Some(SeqItem::Op(op)) if op == "=") {
        index += 1;
        let (right, next) = climb_add(items, index)?;
        left = apply2("=", left, right)?;
        index = next;
    }
    Ok((left, index))
}

pub(super) fn climb_add(
    items: &[SeqItem],
    start: usize,
) -> Result<(BinderTerm, usize), LayoutError> {
    let (mut left, mut index) = climb_mul(items, start)?;
    while let Some(SeqItem::Op(op)) = items.get(index) {
        if op != "+" && op != "-" {
            break;
        }
        let op = op.clone();
        index += 1;
        let (right, next) = climb_mul(items, index)?;
        left = apply2(&op, left, right)?;
        index = next;
    }
    Ok((left, index))
}

pub(super) fn climb_mul(
    items: &[SeqItem],
    start: usize,
) -> Result<(BinderTerm, usize), LayoutError> {
    let (mut left, mut index) = climb_atom(items, start)?;
    loop {
        match items.get(index) {
            Some(SeqItem::Op(op)) if op == "*" || op == "/" => {
                let op = op.clone();
                index += 1;
                let (right, next) = climb_atom(items, index)?;
                left = apply2(&op, left, right)?;
                index = next;
            }
            Some(SeqItem::Term(_)) => {
                let (right, next) = climb_atom(items, index)?;
                left = apply2("*", left, right)?;
                index = next;
            }
            _ => break,
        }
    }
    Ok((left, index))
}

pub(super) fn climb_atom(
    items: &[SeqItem],
    start: usize,
) -> Result<(BinderTerm, usize), LayoutError> {
    match items.get(start) {
        Some(SeqItem::Term(term)) => Ok((clone_term(term), start + 1)),
        Some(SeqItem::Op(op)) => Err(LayoutError::Unlowered {
            reason: format!("expected term, found operator {op}"),
        }),
        None => Err(LayoutError::Unlowered {
            reason: "expected term, found end of sequence".to_string(),
        }),
    }
}

pub(super) fn clone_term(term: &BinderTerm) -> BinderTerm {
    term.clone()
}

pub(super) fn apply2(
    op: &str,
    left: BinderTerm,
    right: BinderTerm,
) -> Result<BinderTerm, LayoutError> {
    match (left, right) {
        (BinderTerm::Leaf(left), BinderTerm::Leaf(right)) => Ok(BinderTerm::Leaf(Term::Apply {
            operator: SymbolId(op.to_string()),
            arguments: vec![left, right],
        })),
        (BinderTerm::Leaf(_), BinderTerm::Bind(binder)) if op == "=" => {
            Ok(BinderTerm::Bind(binder))
        }
        (BinderTerm::Bind(binder), _) => Ok(BinderTerm::Bind(binder)),
        _ => Err(LayoutError::Unlowered {
            reason: format!("cannot apply {op:?} across a binder"),
        }),
    }
}

pub(super) fn glyph_term(text: &str) -> Term {
    if text.chars().all(|ch| ch.is_ascii_digit()) && !text.is_empty() {
        Term::Constant(SymbolId(text.to_string()))
    } else {
        Term::Variable(VariableId(text.to_string()))
    }
}

pub(super) fn is_infix_op(text: &str) -> bool {
    matches!(text, "+" | "-" | "*" | "/" | "=")
}

pub(super) fn as_int(term: &Term) -> Option<i64> {
    match term {
        Term::Constant(symbol) => symbol.0.parse().ok(),
        _ => None,
    }
}

pub(super) fn as_int_binder(term: &BinderTerm) -> Option<i64> {
    match term {
        BinderTerm::Leaf(leaf) => as_int(leaf),
        BinderTerm::Bind(_) => None,
    }
}
