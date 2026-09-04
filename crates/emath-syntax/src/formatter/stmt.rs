//! Statement and type formatting (moved verbatim).

use super::*;

pub(super) fn format_stmt(out: &mut String, stmt: &Stmt, level: usize) {
    indent(out, level);
    if format_stmt_kind(out, &stmt.kind, level) {
        // Nested-block kinds end with their own last-statement/header
        // newline; adding another would leave a blank line after every
        // section, fn body, or if/binder block (SURF-0013).
        return;
    }
    out.push('\n');
}

/// Formats a statement kind; returns `true` when the output already ends
/// with a newline (nested-suite kinds and `Self:` blocks).
pub(super) fn format_stmt_kind(out: &mut String, kind: &StmtKind, level: usize) -> bool {
    let nested = stmt_kind_ends_with_newline(kind);
    format_stmt_kind_inner(out, kind, level);
    nested
}

/// Whether a statement kind ends with its own newline (nested-suite
/// kinds and `Self:` blocks render their terminator internally).
/// One side of a reaction line: coefficient-prefixed species joined by
/// `+`; the default coefficient 1 prints bare.
pub(super) fn format_reaction_side(out: &mut String, terms: &[ReactionTerm]) {
    for (index, term) in terms.iter().enumerate() {
        if index > 0 {
            out.push_str(" + ");
        }
        if term.coefficient != 1 {
            out.push_str(&term.coefficient.to_string());
        }
        out.push_str(&term.species);
    }
}

pub(super) fn stmt_kind_ends_with_newline(kind: &StmtKind) -> bool {
    matches!(
        kind,
        StmtKind::Section(_)
            | StmtKind::If { .. }
            | StmtKind::BinderStmt { .. }
            | StmtKind::SelfBlock { .. }
            | StmtKind::FnDecl { suite: Some(_), .. }
    )
}

pub(super) fn format_stmt_kind_inner(out: &mut String, kind: &StmtKind, level: usize) {
    match kind {
        StmtKind::Section(section) => {
            format_section_head(out, section);
            out.push('\n');
            if section.name == "observations" {
                // 04 §5.2: rows are
                // `obs <name>[: <type>] = <data>`; the tree stores them
                // as `FieldDecl` and the `obs` prefix is section-implied,
                // so it is restored here on output.
                format_observations_suite(out, &section.suite, level + 1);
            } else {
                format_suite(out, &section.suite, level + 1);
            }
        }
        StmtKind::FieldDecl {
            visibility,
            name,
            ty,
            default,
        } => {
            format_visibility(out, *visibility);
            out.push_str(name);
            if !is_infer_marker(ty) {
                out.push_str(": ");
                format_type(out, ty);
            }
            if let Some(default_expr) = default {
                out.push_str(" = ");
                format_expr(out, default_expr, Prec::Root);
            }
        }
        StmtKind::FnDecl {
            visibility,
            head,
            name,
            params,
            ret,
            suite,
            ..
        } => {
            format_visibility(out, *visibility);
            out.push_str(head);
            out.push(' ');
            out.push_str(name);
            format_params(out, params);
            if let Some(ret_ty) = ret {
                out.push_str(" -> ");
                format_type(out, ret_ty);
            }
            match suite {
                Some(body) => {
                    out.push_str(":\n");
                    format_suite(out, body, level + 1);
                }
                // Suite-less fn declarations are not part of the corpus;
                // keep the print total so the tree stays round-trippable
                // until the parser admits them.
                None => out.push(':'),
            }
        }
        StmtKind::OperatorDecl {
            name, params, ret, ..
        } => {
            out.push_str("operator ");
            out.push_str(name);
            format_params(out, params);
            if let Some(ret_ty) = ret {
                out.push_str(" -> ");
                format_type(out, ret_ty);
            }
            out.push(':');
        }
        StmtKind::Let { name, ty, value } => {
            out.push_str("let ");
            out.push_str(name);
            if let Some(ty) = ty {
                out.push_str(": ");
                format_type(out, ty);
            }
            out.push_str(" = ");
            format_expr(out, value, Prec::Root);
        }
        StmtKind::Assign { target, value } => {
            format_place(out, target);
            out.push_str(" = ");
            format_expr(out, value, Prec::Root);
        }
        StmtKind::Require(expr) => {
            out.push_str("require ");
            format_expr(out, expr, Prec::Root);
        }
        StmtKind::Ensure(expr) => {
            out.push_str("ensure ");
            format_expr(out, expr, Prec::Root);
        }
        StmtKind::Invariant(expr) => {
            out.push_str("invariant ");
            format_expr(out, expr, Prec::Root);
        }
        StmtKind::Given { name, value } => {
            out.push_str("given ");
            out.push_str(name);
            out.push_str(" = ");
            format_expr(out, value, Prec::Root);
        }
        StmtKind::Expect(expr) => {
            out.push_str("expect ");
            format_expr(out, expr, Prec::Root);
        }
        StmtKind::Expr(expr) => format_expr(out, expr, Prec::Root),
        StmtKind::If {
            condition,
            then,
            else_branches,
            else_tail,
        } => {
            out.push_str("if ");
            format_expr(out, condition, Prec::Root);
            out.push_str(":\n");
            format_suite(out, then, level + 1);
            for (idx_expr, branch) in else_branches.iter().enumerate() {
                if !branch.1.statements.is_empty() || idx_expr == 0 {
                    indent(out, level);
                    out.push_str("else if ");
                    format_expr(out, &branch.0, Prec::Root);
                    out.push_str(":\n");
                    format_suite(out, &branch.1, level + 1);
                }
            }
            if let Some(tail) = else_tail {
                if !tail.statements.is_empty() {
                    indent(out, level);
                    out.push_str("else:\n");
                    format_suite(out, tail, level + 1);
                }
            }
        }
        StmtKind::BinderStmt {
            kind: bkind,
            binders,
            suite,
            guard,
        } => {
            format_binder_head(out, *bkind, binders);
            if let Some(guard_expr) = guard {
                out.push_str(" if ");
                format_expr(out, guard_expr, Prec::Root);
            }
            out.push_str(":\n");
            format_suite(out, suite, level + 1);
        }
        StmtKind::SelfBlock { assignments } => {
            out.push_str("Self:");
            for (name, expr) in assignments {
                out.push('\n');
                indent(out, level + 1);
                out.push_str(name);
                out.push_str(" = ");
                format_expr(out, expr, Prec::Root);
            }
            out.push('\n');
        }
        // SURF-0013: `format_stmt` already emitted the single level
        // indent and the trailing newline; this arm must not indent or
        // terminate again (that double-indented equations and left a
        // blank line after every one).
        StmtKind::Equation { left, right } => {
            format_expr(out, left, Prec::Comparison);
            out.push_str(" = ");
            format_expr(out, right, Prec::Comparison);
        }
        StmtKind::Reaction {
            name,
            lhs,
            arrow,
            rhs,
        } => {
            out.push_str(name);
            out.push_str(": ");
            format_reaction_side(out, lhs);
            out.push(' ');
            out.push_str(arrow.as_str());
            out.push(' ');
            format_reaction_side(out, rhs);
        }
        StmtKind::Reaction {
            name,
            lhs,
            arrow,
            rhs,
        } => {
            out.push_str(name);
            out.push_str(": ");
            format_reaction_side(out, lhs);
            out.push(' ');
            out.push_str(arrow.as_str());
            out.push(' ');
            format_reaction_side(out, rhs);
        }
        StmtKind::Command { head, argument } => {
            out.push_str(&head.join(" "));
            match argument {
                Some(CommandArgument::Expr(expr)) => {
                    out.push(' ');
                    format_expr(out, expr, Prec::Root);
                }
                Some(CommandArgument::Assignment { name, value }) => {
                    out.push(' ');
                    out.push_str(name);
                    out.push_str(" = ");
                    format_expr(out, value, Prec::Root);
                }
                Some(CommandArgument::List(items)) => {
                    out.push(' ');
                    for (i, item) in items.iter().enumerate() {
                        if i > 0 {
                            out.push_str(", ");
                        }
                        format_expr(out, item, Prec::Root);
                    }
                }
                None => {}
            }
        }
    }
}

pub(super) fn format_visibility(out: &mut String, visibility: Option<Visibility>) {
    match visibility {
        Some(Visibility::Public) => out.push_str("public "),
        Some(Visibility::Package) => out.push_str("package "),
        Some(Visibility::Private) => out.push_str("private "),
        None => {}
    }
}

pub(super) fn format_params(out: &mut String, params: &[Param]) {
    out.push('(');
    for (i, param) in params.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        if param.by_ref {
            out.push('&');
        }
        out.push_str(&param.name);
        out.push_str(": ");
        format_type(out, &param.ty);
        if let Some(default_expr) = &param.default {
            out.push_str(" = ");
            format_expr(out, default_expr, Prec::Root);
        }
    }
    out.push(')');
}

pub(super) fn format_place(out: &mut String, place: &Place) {
    out.push_str(&place.segments.join("::"));
    for index in &place.indices {
        out.push('[');
        format_expr(out, index, Prec::Atomic);
        out.push(']');
    }
}

pub(super) fn binder_kind_str(kind: BinderKind) -> &'static str {
    match kind {
        BinderKind::Sum => "sum",
        BinderKind::Product => "product",
        BinderKind::Integral => "integral",
        BinderKind::ForAll => "forall",
        BinderKind::Exists => "exists",
        BinderKind::Series => "series",
    }
}

/// `forall x in 0..B, t in 0..T`: the binder head has no parens; the
/// grammar's binder list is comma-separated after the keyword.
pub(super) fn format_binder_head(out: &mut String, kind: BinderKind, binders: &[Binder]) {
    out.push_str(binder_kind_str(kind));
    out.push(' ');
    for (i, binder) in binders.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&binder.name);
        if let Some(domain) = &binder.domain {
            out.push_str(" in ");
            format_expr(out, domain, Prec::Root);
        }
    }
}

pub(super) fn is_infer_marker(ty: &TypeExpr) -> bool {
    matches!(
        &ty.kind,
        TypeKind::Path {
            segments,
            generic_args,
        } if generic_args.is_empty() && segments.last().map(String::as_str) == Some("Infer")
    )
}

/// Format a type expression. Types have no precedence ambiguity in the
/// supported surface, so no parenthesization is needed.
pub fn format_type(out: &mut String, ty: &TypeExpr) {
    match &ty.kind {
        TypeKind::Path {
            segments,
            generic_args,
        } => {
            out.push_str(&segments.join("::"));
            if !generic_args.is_empty() {
                out.push('<');
                for (i, arg) in generic_args.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    format_generic_arg(out, arg);
                }
                out.push('>');
            }
        }
        TypeKind::List(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                format_type(out, item);
            }
            out.push(']');
        }
        TypeKind::Tuple(items) => {
            out.push('(');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                format_type(out, item);
            }
            out.push(')');
        }
        TypeKind::Ref(inner) => {
            out.push('&');
            format_type(out, inner);
        }
        TypeKind::Product { left, op, right } => {
            format_type(out, left);
            out.push_str(op.as_str());
            format_type(out, right);
        }
        TypeKind::Pow { base, exponent } => {
            if matches!(base.kind, TypeKind::Product { .. }) {
                out.push('(');
                format_type(out, base);
                out.push(')');
            } else {
                format_type(out, base);
            }
            out.push('^');
            out.push_str(&exponent.to_string());
        }
        TypeKind::In { base, unit } => {
            format_type(out, base);
            out.push_str(" in ");
            format_type(out, unit);
        }
        TypeKind::Domain { base, lo, hi } => {
            format_type(out, base);
            out.push_str(" in [");
            expr::format_expr_inner(out, lo);
            out.push_str(", ");
            expr::format_expr_inner(out, hi);
            out.push(']');
        }
    }
}

pub fn format_generic_arg(out: &mut String, arg: &GenericArg) {
    match arg {
        GenericArg::Type(ty) => format_type(out, ty),
        GenericArg::Value(expr) => format_expr(out, expr, Prec::Root),
        GenericArg::Named { name, arg } => {
            out.push_str(name);
            out.push_str(" = ");
            format_generic_arg(out, arg);
        }
    }
}
