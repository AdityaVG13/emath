//! Canonical formatter.
//!
//! Idempotent, comment-preserving printer over the `SyntaxTree`. Layout is
//! canonical (4-space indents, one statement per line); parentheses are
//! re-introduced from the precedence table, so `parse(format(x))` equals
//! `parse(x)` for every lossless-parsable `x`. Comments are retained at
//! line-lead or trailing positions by span order.

use crate::token::Comment;
use crate::tree::{
    Argument, ArgumentValue, Attribute, BinaryOp, Binder, BinderKind, CommandArgument, Declaration,
    Expr, ExprKind, Item, Param, Place, Section, Stmt, StmtKind, Suite, SyntaxTree, TypeExpr,
    TypeKind, UnaryOp, UseTree, Visibility,
};

/// Precedence levels matching the parser's chain
/// (or < and < comparison < additive < multiplicative < power < unary).
/// `Root` is the statement level: nothing is parenthesized there.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Prec {
    Root,
    Or,
    And,
    Comparison,
    Additive,
    Multiplicative,
    Power,
    Unary,
    Atomic,
}

#[must_use]
pub fn binary_prec(op: BinaryOp) -> Prec {
    match op {
        BinaryOp::Or => Prec::Or,
        BinaryOp::And => Prec::And,
        BinaryOp::Eq | BinaryOp::Ne | BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => {
            Prec::Comparison
        }
        BinaryOp::Add | BinaryOp::Sub => Prec::Additive,
        BinaryOp::Mul | BinaryOp::Div => Prec::Multiplicative,
        BinaryOp::Pow => Prec::Power,
    }
}

#[must_use]
pub fn unary_prec(_op: UnaryOp) -> Prec {
    Prec::Unary
}

/// Format the tree. `comments` come from `lex_with_comments`.
#[must_use]
pub fn format(tree: &SyntaxTree, comments: &[Comment]) -> String {
    let mut out = String::new();
    let mut emitted: Vec<usize> = Vec::new();
    let comments: Vec<&Comment> = comments.iter().collect();
    for item in &tree.items {
        emit_lead_comments(&mut out, &comments, &mut emitted, item_span_start(item));
        format_item(&mut out, item, 0);
        out.push('\n');
        out.push('\n');
    }
    emit_lead_comments(&mut out, &comments, &mut emitted, usize::MAX);
    // Trailing comments (attached after code in the source) are retained,
    // emitted after the item that contains them (or at EOF) in span order.
    for comment in &comments {
        if comment.own_line {
            continue;
        }
        out.push_str(&comment.text);
        out.push('\n');
    }
    out
}

fn item_span_start(item: &Item) -> usize {
    match item {
        Item::Package { source, .. } | Item::Use { source, .. } => source.start as usize,
        Item::Declaration(decl) => decl.head_source.start as usize,
    }
}

fn emit_lead_comments(
    out: &mut String,
    comments: &[&Comment],
    emitted: &mut Vec<usize>,
    before: usize,
) {
    for (idx, comment) in comments.iter().enumerate().skip(emitted.len()) {
        if !comment.own_line {
            continue;
        }
        let start = comment.span.start as usize;
        if start >= before {
            break;
        }
        out.push_str(&comment.text);
        out.push('\n');
        emitted.push(idx);
    }
}

fn indent(out: &mut String, level: usize) {
    for _ in 0..level {
        out.push_str("    ");
    }
}

fn format_item(out: &mut String, item: &Item, level: usize) {
    match item {
        Item::Package { path, .. } => {
            indent(out, level);
            out.push_str("package ");
            out.push_str(&path.join("."));
            out.push('\n');
        }
        Item::Use { path, tree, .. } => {
            indent(out, level);
            out.push_str("use ");
            out.push_str(&path.join("::"));
            match tree {
                UseTree::All => out.push_str("::*"),
                UseTree::Named(names) => {
                    out.push_str("::{");
                    for (i, (name, alias)) in names.iter().enumerate() {
                        if i > 0 {
                            out.push_str(", ");
                        }
                        out.push_str(name);
                        if let Some(alias) = alias {
                            out.push_str(" as ");
                            out.push_str(alias);
                        }
                    }
                    out.push('}');
                }
            }
        }
        Item::Declaration(decl) => format_declaration(out, decl, level),
    }
}

fn format_declaration(out: &mut String, decl: &Declaration, level: usize) {
    // `emath <item_kind> <<name>[<generics>]> [as <as_kind>] :`
    indent(out, level);
    out.push_str("emath ");
    out.push_str(&decl.item_kind);
    out.push_str(" <");
    out.push_str(&decl.name);
    if !decl.generics.is_empty() {
        out.push('<');
        for (i, generic) in decl.generics.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            out.push_str(&generic.name);
            if let Some(bound) = &generic.bound {
                out.push_str(": ");
                format_type(out, bound);
            }
        }
        out.push('>');
    }
    out.push('>');
    if !decl.as_kind.is_empty() {
        out.push_str(" as ");
        out.push_str(&decl.as_kind);
    }
    if let Some(signature) = &decl.signature {
        out.push('(');
        for (i, param) in signature.params.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            out.push_str(&param.name);
            out.push_str(": ");
            format_type(out, &param.ty);
        }
        out.push(')');
        if let Some(ret) = &signature.ret {
            out.push_str(" -> ");
            format_type(out, ret);
        }
    }
    out.push(':');
    out.push('\n');
    for attribute in &decl.attributes {
        format_attribute(out, attribute, level + 1);
        out.push('\n');
    }
    // Body statements (sections, fn-like heads, and other statements) in
    // source order, separated by blank lines at declaration level.
    for stmt in &decl.body {
        format_stmt(out, stmt, level + 1);
        out.push('\n');
    }
}

fn format_attribute(out: &mut String, attribute: &Attribute, level: usize) {
    indent(out, level);
    out.push('[');
    out.push_str(&attribute.name);
    if !attribute.args.is_empty() {
        out.push('(');
        for (i, arg) in attribute.args.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            out.push_str(arg);
        }
        out.push(')');
    }
    out.push(']');
}

fn format_section_head(out: &mut String, section: &Section) {
    out.push_str(&section.name);
    if let Some(generic) = &section.generic {
        out.push('<');
        out.push_str(generic);
        out.push('>');
    }
    if let Some(args) = &section.args {
        out.push(' ');
        for (i, arg) in args.iter().enumerate() {
            if i > 0 {
                out.push(' ');
            }
            format_argument(out, arg);
        }
    }
    out.push(':');
}

fn format_argument(out: &mut String, argument: &Argument) {
    if let Some(name) = &argument.name {
        out.push_str(name);
        out.push_str(" = ");
    }
    match &argument.value {
        ArgumentValue::Expr(expr) => format_expr(out, expr, Prec::Root),
        ArgumentValue::Type(ty) => format_type(out, ty),
    }
}

fn format_suite(out: &mut String, suite: &Suite, level: usize) {
    for stmt in &suite.statements {
        format_stmt(out, stmt, level);
    }
}

fn format_stmt(out: &mut String, stmt: &Stmt, level: usize) {
    indent(out, level);
    format_stmt_kind(out, &stmt.kind, level);
    out.push('\n');
}

fn format_stmt_kind(out: &mut String, kind: &StmtKind, level: usize) {
    match kind {
        StmtKind::Section(section) => {
            format_section_head(out, section);
            out.push('\n');
            format_suite(out, &section.suite, level + 1);
        }
        StmtKind::FieldDecl {
            visibility,
            name,
            ty,
            default,
        } => {
            format_visibility(out, *visibility);
            out.push_str(name);
            out.push_str(": ");
            format_type(out, ty);
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
        } => {
            format_binder_head(out, *bkind, binders);
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
        StmtKind::Equation { left, right } => {
            indent(out, level);
            format_expr(out, left, Prec::Comparison);
            out.push_str(" = ");
            format_expr(out, right, Prec::Comparison);
            out.push('\n');
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

fn format_visibility(out: &mut String, visibility: Option<Visibility>) {
    match visibility {
        Some(Visibility::Public) => out.push_str("public "),
        Some(Visibility::Package) => out.push_str("package "),
        Some(Visibility::Private) => out.push_str("private "),
        None => {}
    }
}

fn format_params(out: &mut String, params: &[Param]) {
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

fn format_place(out: &mut String, place: &Place) {
    out.push_str(&place.segments.join("::"));
    for index in &place.indices {
        out.push('[');
        format_expr(out, index, Prec::Atomic);
        out.push(']');
    }
}

fn binder_kind_str(kind: BinderKind) -> &'static str {
    match kind {
        BinderKind::Sum => "sum",
        BinderKind::Product => "product",
        BinderKind::Integral => "integral",
        BinderKind::ForAll => "forall",
        BinderKind::Exists => "exists",
    }
}

/// `forall x in 0..B, t in 0..T`: the binder head has no parens; the
/// grammar's binder list is comma-separated after the keyword.
fn format_binder_head(out: &mut String, kind: BinderKind, binders: &[Binder]) {
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
                    format_type(out, arg);
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
        TypeKind::Product(items) => {
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(" * ");
                }
                format_type(out, item);
            }
        }
    }
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
        | ExprKind::Derivative { .. } => true,
        // Binder expressions (`sum(i in S) body`) parse greedily; parens
        // keep them scoped inside larger factors, and the body must never
        // be parenthesized (see the binder arm below).
        ExprKind::Binder { .. } => parent > Prec::Atomic,
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

fn format_expr_inner(out: &mut String, expr: &Expr) {
    match &expr.kind {
        ExprKind::Int(text) | ExprKind::Float(text) => out.push_str(text),
        ExprKind::Str(text) => {
            out.push('"');
            out.push_str(text);
            out.push('"');
        }
        ExprKind::Bool(value) => out.push_str(if *value { "true" } else { "false" }),
        ExprKind::Quantity { value, unit } => {
            let inner = format_expr_to_string(value);
            out.push_str(&inner);
            out.push(' ');
            out.push_str(&unit.join("::"));
        }
        ExprKind::Path { segments, generics } => {
            out.push_str(&segments.join("::"));
            if let Some(generics) = generics {
                out.push('<');
                for (i, generic) in generics.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    format_type(out, generic);
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
                format_expr(out, arg, Prec::Atomic);
            }
            out.push(')');
        }
        ExprKind::Index { value, indices } => {
            format_expr(out, value, Prec::Root);
            out.push('[');
            for (i, index) in indices.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                format_expr(out, index, Prec::Atomic);
            }
            out.push(']');
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
            format_expr(out, left, prec);
            out.push(' ');
            out.push_str(binary_spelling(*op));
            out.push(' ');
            format_expr(out, right, prec);
        }
        ExprKind::If {
            condition,
            then_value,
            else_value,
        } => {
            out.push_str("if ");
            format_expr(out, condition, Prec::Root);
            out.push_str(" then ");
            format_expr(out, then_value, Prec::Atomic);
            out.push_str(" else ");
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
            out.push(')');
        }
        ExprKind::Range {
            start,
            end,
            inclusive,
        } => {
            if let Some(start) = start {
                format_expr(out, start, Prec::Root);
            }
            out.push_str(if *inclusive { "..=" } else { ".." });
            if let Some(end) = end {
                format_expr(out, end, Prec::Root);
            }
        }
        ExprKind::Binder {
            kind,
            binders,
            body,
        } => {
            // The expression-level binder requires the colon form:
            // `sum i in S: body`.
            format_binder_head(out, *kind, binders);
            out.push_str(": ");
            format_expr(out, body, Prec::Root);
        }
        ExprKind::Derivative { value, wrt } => {
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
    }
}

fn format_expr_to_string(expr: &Expr) -> String {
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
    }
}
