//! Section admission helpers: integer ranges and binder restore,
//! extracted from `admit.rs` isomorphically.

use emath_core::tree::{Expr, ExprKind};
use std::collections::BTreeMap;

use super::expr_number;
use super::infer::Infer;

pub(super) fn integer_range(expr: &Expr) -> Option<(i64, i64)> {
    let ExprKind::Range {
        start,
        end,
        inclusive,
    } = &expr.kind
    else {
        return None;
    };
    let start = start.as_ref().and_then(|expr| integer_bound(expr))?;
    let end = end.as_ref().and_then(|expr| integer_bound(expr))?;
    let end = if *inclusive { end.checked_add(1)? } else { end };
    Some((start, end))
}

pub(super) fn integer_bound(expr: &Expr) -> Option<i64> {
    let value = expr_number(expr)?;
    if !value.is_finite() || value.fract() != 0.0 {
        return None;
    }
    Some(value as i64)
}

pub(super) fn restore_index_local(
    locals: &mut BTreeMap<String, i64>,
    name: &str,
    previous: Option<i64>,
) {
    match previous {
        Some(value) => {
            locals.insert(name.to_string(), value);
        }
        None => {
            locals.remove(name);
        }
    }
}

pub(super) fn restore_input(
    locals: &mut BTreeMap<String, Infer>,
    name: &str,
    previous: Option<Infer>,
) {
    match previous {
        Some(infer) => {
            locals.insert(name.to_string(), infer);
        }
        None => {
            locals.remove(name);
        }
    }
}
