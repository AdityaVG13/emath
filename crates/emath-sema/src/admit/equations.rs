//! Equation admission helpers: state-name resolution and derivative
//! detection, extracted from `admit.rs` isomorphically.

use emath_core::tree::{
    Expr, ExprKind,
};

use super::Admitter;


pub(super) fn state_variable_name(admitter: &Admitter, segments: &[String], name: &str) -> String {
    if name.starts_with("state.") {
        return name.to_string();
    }
    if segments.len() == 1
        && admitter.states.contains_key(&segments[0])
        && !admitter.inputs.contains_key(&segments[0])
        && !admitter.params.contains_key(&segments[0])
        && !admitter.definitions.contains_key(&segments[0])
    {
        return format!("state.{}", segments[0]);
    }
    name.to_string()
}

pub(super) fn path_segments(expr: &Expr) -> Option<&[String]> {
    match &expr.kind {
        ExprKind::Path { segments, .. } => Some(segments),
        _ => None,
    }
}

pub(super) fn is_der_call(function: &Expr) -> bool {
    path_segments(function).is_some_and(|segments| {
        segments.len() == 1 && matches!(segments[0].as_str(), "der" | "derivative")
    })
}

/// Explicit `derivative(state)` / `der(state)` as an ordinary call.
pub(super) fn unwrap_derivative(expr: &Expr) -> Option<(&Expr, Option<&[Expr]>)> {
    match &expr.kind {
        ExprKind::Call { function, args } if args.len() == 1 && is_der_call(function) => {
            Some((&args[0], None))
        }
        _ => None,
    }
}
