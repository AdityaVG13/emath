//! Expression formatting.
//!
//! Extracted from the parent formatter module. Formats expression AST
//! nodes into source text, applying precedence-based parenthesization.

use super::Prec;
use super::format_binder_head;
use crate::tree::{BinaryOp, Expr, ExprKind, LimitDirection, UnaryOp};

#[must_use]
pub fn binary_prec(op: BinaryOp) -> Prec {
    match op {
        BinaryOp::Iff => Prec::Iff,
        BinaryOp::Imply => Prec::Imply,
        BinaryOp::Or => Prec::Or,
        BinaryOp::And => Prec::And,
        BinaryOp::Eq | BinaryOp::Ne | BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => {
            Prec::Comparison
        }
        BinaryOp::Asymp => Prec::Comparison,
        BinaryOp::Add | BinaryOp::Sub => Prec::Additive,
        BinaryOp::Mul | BinaryOp::Div => Prec::Multiplicative,
        BinaryOp::Pow => Prec::Power,
    }
}

#[must_use]
pub fn unary_prec(_op: UnaryOp) -> Prec {
    Prec::Unary
}

/// Format an expression, parenthesizing children whose precedence binds
/// looser than the containing operator.
pub fn format_expr(out: &mut String, expr: &Expr, parent: Prec) {
    let needs_parens = match &expr.kind {
        ExprKind::Binary { op, .. } => binary_prec(*op) < parent,
        // `(-x) ^ 2` must keep its parens; next to any binary/factor
        // operator the unary prefix re-associates differently.
        ExprKind::Unary { .. } => parent >= Prec::Power,
        // Postfix clauses (`at`, `on`, `if`, `derivative ... wrt ...`)
        // are only consumed at depth > 0, so parenthesize always: the
        // formatter output is position- and depth-independent.
        ExprKind::At { .. }
        | ExprKind::On { .. }
        | ExprKind::Conditioned { .. }
        | ExprKind::Derivative { .. }
        | ExprKind::Solve { .. }
        | ExprKind::Optimize { .. }
        | ExprKind::UnitQuery { .. } => true,
        // Colon-greedy forms (`sum i in S: body`, `if c: a else: b`,
        // `limit x -> 0: body`, `cases ...`). Without parens the body
        // swallows a following operator: `(sum i in S: i) * x` must not
        // print as `sum i in S: i * x`.
        ExprKind::Binder { .. }
        | ExprKind::Limit { .. }
        | ExprKind::SampleLimit { .. }
        | ExprKind::Cases { .. }
        | ExprKind::If { .. } => parent > Prec::Root,
        _ => false,
    };
    if needs_parens {
        out.push('(');
    }
    format_expr_inner(out, expr);
    if needs_parens {
        out.push(')');
    }
}

pub(super) fn format_expr_inner(out: &mut String, expr: &Expr) {
    match &expr.kind {
        ExprKind::Int(text) | ExprKind::Float(text) => out.push_str(text),
        ExprKind::Rational { numer, denom } => {
            out.push_str(numer);
            out.push_str("//");
            out.push_str(denom);
        }
        ExprKind::Str(text) => super::push_string_literal(out, text),
        ExprKind::Bool(value) => out.push_str(if *value { "true" } else { "false" }),
        ExprKind::Quantity { value, unit } => {
            let inner = format_expr_to_string(value);
            out.push_str(&inner);
            if unit.is_simple() {
                // Simple unit: `9.81 m`
                out.push(' ');
                out.push_str(&unit.to_string());
            } else {
                // Compound unit: `9.81 [unit m/s^2]` (canonical form)
                out.push_str(" [unit ");
                out.push_str(&unit.canonical_form());
                out.push(']');
            }
        }
        ExprKind::Path { segments, generics } => {
            // Corpus-canonical separator for expression paths is `.`
            // (`state.scale`); the parser accepts both `.` and `::` into
            // the same segments, so this render reparses identically.
            out.push_str(&segments.join("."));
            if let Some(generics) = generics {
                out.push('<');
                for (i, generic) in generics.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    super::format_generic_arg(out, generic);
                }
                out.push('>');
            }
        }
        ExprKind::Call { function, args } => {
            format_expr(out, function, Prec::Atomic);
            out.push('(');
            for (i, arg) in args.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                format_expr(out, arg, Prec::Root);
            }
            out.push(')');
        }
        ExprKind::Index { value, indices } => {
            format_expr(out, value, Prec::Atomic);
            out.push('[');
            for (i, index) in indices.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                format_expr(out, index, Prec::Root);
            }
            out.push(']');
        }
        ExprKind::Slice { start, end } => {
            if let Some(start) = start {
                format_expr(out, start, Prec::Root);
            }
            out.push(':');
            if let Some(end) = end {
                format_expr(out, end, Prec::Root);
            }
        }
        ExprKind::Unary { op, value } => {
            match op {
                UnaryOp::Neg => out.push('-'),
                UnaryOp::Pos => out.push('+'),
                UnaryOp::Not => out.push_str("not "),
            }
            format_expr(out, value, unary_prec(*op));
        }
        ExprKind::Binary { op, left, right } => {
            let prec = binary_prec(*op);
            // Same-prec children need an extra tick of tightness on the
            // non-associating side: `a - (b - c)` must not print as
            // `a - b - c`, and `(a ^ b) ^ c` must keep the left parens.
            let (left_prec, right_prec) = if matches!(*op, BinaryOp::Pow | BinaryOp::Imply) {
                (prec.tighter(), prec)
            } else {
                (prec, prec.tighter())
            };
            format_expr(out, left, left_prec);
            out.push(' ');
            out.push_str(binary_spelling(*op));
            out.push(' ');
            format_expr(out, right, right_prec);
        }
        ExprKind::If {
            condition,
            then_value,
            else_value,
        } => {
            // Grammar: `if` expression `:` expression `else` `:` expression.
            // Printing `then` was not a keyword and failed to reparse.
            out.push_str("if ");
            format_expr(out, condition, Prec::Root);
            out.push_str(": ");
            format_expr(out, then_value, Prec::Atomic);
            out.push_str(" else: ");
            format_expr(out, else_value, Prec::Atomic);
        }
        ExprKind::List(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                format_expr(out, item, Prec::Root);
            }
            out.push(']');
        }
        ExprKind::Tuple(items) => {
            out.push('(');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                format_expr(out, item, Prec::Root);
            }
            // `(x)` is grouping, not a 1-tuple; keep the trailing comma.
            if items.len() == 1 {
                out.push(',');
            }
            out.push(')');
        }
        ExprKind::Range {
            start,
            end,
            inclusive,
        } => {
            if let Some(start) = start {
                format_expr(out, start, Prec::Atomic);
            }
            out.push_str(if *inclusive { "..=" } else { ".." });
            if let Some(end) = end {
                format_expr(out, end, Prec::Atomic);
            }
        }
        ExprKind::Binder {
            kind,
            binders,
            body,
            guard,
        } => {
            // The expression-level binder requires the colon form:
            // `sum i in S: body` or `sum i in S if cond: body`.
            format_binder_head(out, *kind, binders);
            if let Some(guard_expr) = guard {
                out.push_str(" if ");
                format_expr(out, guard_expr, Prec::Root);
            }
            out.push_str(": ");
            format_expr(out, body, Prec::Root);
        }
        ExprKind::Derivative {
            value,
            wrt,
            kind,
            holding,
        } => {
            match kind {
                crate::tree::DerivativeKind::Plain => out.push_str("derivative "),
                crate::tree::DerivativeKind::Partial => out.push_str("partial "),
                crate::tree::DerivativeKind::Total => out.push_str("total "),
            }
            // Operand is a postfix_expr (`derivative (v + v)`); Root
            // would drop the grouping and reparse as `(derivative v) + v`.
            format_expr(out, value, Prec::Atomic);
            if let Some(items) = wrt {
                out.push_str(" wrt ");
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    format_expr(out, item, Prec::Root);
                }
            }
            if !holding.is_empty() {
                out.push_str(" holding ");
                for (i, item) in holding.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    format_expr(out, item, Prec::Root);
                }
            }
        }
        ExprKind::Solve { value, wrt } => {
            out.push_str("solve ");
            format_expr(out, value, Prec::Root);
            if let Some(items) = wrt {
                out.push_str(" wrt ");
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    format_expr(out, item, Prec::Root);
                }
            }
        }
        ExprKind::Optimize {
            value,
            wrt,
            maximize,
        } => {
            out.push_str(if *maximize { "maximize " } else { "minimize " });
            format_expr(out, value, Prec::Root);
            if let Some(items) = wrt {
                out.push_str(" wrt ");
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    format_expr(out, item, Prec::Root);
                }
            }
        }
        ExprKind::At { value, location } => {
            format_expr(out, value, Prec::Root);
            out.push_str(" at ");
            format_expr(out, location, Prec::Root);
        }
        ExprKind::On { value, location } => {
            format_expr(out, value, Prec::Root);
            out.push_str(" on ");
            format_expr(out, location, Prec::Root);
        }
        ExprKind::Conditioned { value, condition } => {
            format_expr(out, value, Prec::Root);
            out.push_str(" if ");
            format_expr(out, condition, Prec::Root);
        }
        ExprKind::UnitQuery { kind, expr } => {
            match kind {
                crate::tree::UnitQueryKind::Unit => out.push_str("unit of "),
                crate::tree::UnitQueryKind::Dimension => out.push_str("dimension of "),
            }
            format_expr(out, expr, Prec::Root);
        }
        ExprKind::Limit {
            var,
            target,
            direction,
            body,
        } => {
            out.push_str("limit ");
            out.push_str(var);
            out.push_str(" -> ");
            format_expr(out, target, Prec::Multiplicative);
            match direction {
                LimitDirection::TwoSided => {}
                LimitDirection::FromAbove => out.push('+'),
                LimitDirection::FromBelow => out.push('-'),
            }
            out.push_str(": ");
            format_expr(out, body, Prec::Root);
        }
        ExprKind::SampleLimit {
            var,
            target,
            direction,
            body,
        } => {
            out.push_str("sample_limit ");
            out.push_str(var);
            out.push_str(" -> ");
            format_expr(out, target, Prec::Multiplicative);
            match direction {
                LimitDirection::TwoSided => {}
                LimitDirection::FromAbove => out.push('+'),
                LimitDirection::FromBelow => out.push('-'),
            }
            out.push_str(": ");
            format_expr(out, body, Prec::Root);
        }
        ExprKind::Cases {
            subject,
            arms,
            else_arm,
        } => {
            out.push_str("cases ");
            if let Some(subj) = subject {
                format_expr(out, subj, Prec::Root);
                out.push_str(": ");
            }
            for (cond, val) in arms {
                out.push_str("| ");
                format_expr(out, cond, Prec::Root);
                out.push_str(" => ");
                format_expr(out, val, Prec::Root);
                out.push(' ');
            }
            out.push_str("| else => ");
            format_expr(out, else_arm, Prec::Root);
        }
    }
}

pub(super) fn format_expr_to_string(expr: &Expr) -> String {
    let mut out = String::new();
    format_expr_inner(&mut out, expr);
    out.trim().to_string()
}

#[must_use]
pub fn binary_spelling(op: BinaryOp) -> &'static str {
    match op {
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
        BinaryOp::Div => "/",
        BinaryOp::Pow => "^",
        BinaryOp::Eq => "==",
        BinaryOp::Ne => "!=",
        BinaryOp::Lt => "<",
        BinaryOp::Le => "<=",
        BinaryOp::Gt => ">",
        BinaryOp::Ge => ">=",
        BinaryOp::And => "and",
        BinaryOp::Or => "or",
        BinaryOp::Imply => "==>",
        BinaryOp::Iff => "<==>",
        BinaryOp::Asymp => "~~",
    }
}
