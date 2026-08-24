//! Canonical formatter.
//!
//! Idempotent, comment-preserving printer over the `SyntaxTree`. Layout is
//! canonical (4-space indents, one statement per line); parentheses are
//! re-introduced from the precedence table, so `parse(format(x))` equals
//! `parse(x)` for every lossless-parsable `x`. Comments are retained at
//! line-lead or trailing positions by span order.

use crate::token::Comment;
use crate::tree::{
    Argument, ArgumentValue, Attribute, Binder, BinderKind, CommandArgument, Declaration,
    Item, NotationDecl, NotationFixity, Param, Place, Section, Stmt, StmtKind, Suite,
    SyntaxTree, TypeExpr, TypeKind, UseTree, Visibility,
};

mod expr;
pub use expr::{binary_prec, binary_spelling, format_expr, unary_prec};

/// Precedence levels matching the parser's chain
/// (iff < imply < or < and < comparison < additive < multiplicative < power < unary).
/// `Root` is the statement level: nothing is parenthesized there.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Prec {
    Root,
    Iff,
    Imply,
    Or,
    And,
    Comparison,
    Additive,
    Multiplicative,
    Power,
    Unary,
    Atomic,
}

/// Format the tree. `comments` come from `lex_with_comments`.
#[must_use]
pub fn format(tree: &SyntaxTree, comments: &[Comment]) -> String {
    let mut out = String::new();
    let mut emitted: Vec<usize> = Vec::new();
    let comments: Vec<&Comment> = comments.iter().collect();
    // Items are separated by one blank line; the last item ends with a
    // single newline (no trailing blank lines, SURF-0013).
    for (index, item) in tree.items.iter().enumerate() {
        emit_lead_comments(&mut out, &comments, &mut emitted, item_span_start(item));
        format_item(&mut out, item, 0);
        if index + 1 < tree.items.len() {
            out.push('\n');
            out.push('\n');
        }
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
        Item::Notation(notation) => notation.source.start as usize,
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
        Item::Notation(notation) => format_notation(out, notation, level),
    }
}

fn format_notation(out: &mut String, notation: &NotationDecl, level: usize) {
    indent(out, level);
    out.push_str("notation ");
    let fixity = match notation.fixity {
        NotationFixity::Prefix => "prefix",
        NotationFixity::Postfix => "postfix",
        NotationFixity::InfixLeft => "infixl",
        NotationFixity::InfixRight => "infixr",
        NotationFixity::Infix => "infix",
    };
    out.push_str(fixity);
    out.push(' ');
    out.push_str(&notation.precedence.to_string());
    out.push(' ');
    push_string_literal(out, &notation.glyph);
    out.push_str(" => ");
    out.push_str(&notation.target.join("::"));
    if let Some(alias) = &notation.alias {
        out.push_str(" alias ");
        push_string_literal(out, alias);
    }
    out.push('\n');
}

fn push_string_literal(out: &mut String, text: &str) {
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            _ => out.push(ch),
        }
    }
    out.push('"');
}

fn format_declaration(out: &mut String, decl: &Declaration, level: usize) {
    // SURF-0013 parse-back: heads are rendered exactly in the form the
    // parser accepts. The tree canonicalizes every unified kind onto the
    // `custom` compat lane (`item_kind="custom"`, original kind in
    // `as_kind`), so rendering reverses that: `emath function P:`.
    indent(out, level);
    if decl.item_kind == "extern" {
        // `extern operator name<Generics>(params) -> Ret:` — no `emath`
        // prefix; the parser dispatches on the `extern` keyword.
        out.push_str("extern ");
    } else {
        out.push_str("emath ");
        if decl.as_kind.is_empty() {
            out.push_str("custom");
        } else {
            out.push_str(&decl.as_kind);
        }
        out.push(' ');
    }
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
    if let Some(signature) = &decl.signature {
        out.push('(');
        for (i, param) in signature.params.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            out.push_str(&param.name);
            if !is_infer_marker(&param.ty) {
                out.push_str(": ");
                format_type(out, &param.ty);
            }
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
    // source order, separated by ONE blank line at declaration level —
    // never a trailing blank line after the last statement (SURF-0013).
    for (index, stmt) in decl.body.iter().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        format_stmt(out, stmt, level + 1);
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

/// Section names whose parser dispatch requires the bare two-word head
/// (`record Foo:`, `type Alias:`, `implement path.for.target:`). Every
/// other section renders its generic in angle brackets
/// (`evaluate <score>:`, `example <three_squared>:`), which the angle-head
/// branch reparses identically.
const BARE_GENERIC_SECTIONS: [&str; 7] = [
    "record",
    "variant",
    "trait",
    "implementation",
    "predicate",
    "type",
    "implement",
];

fn format_section_head(out: &mut String, section: &Section) {
    out.push_str(&section.name);
    if let Some(generic) = &section.generic {
        if BARE_GENERIC_SECTIONS.contains(&section.name.as_str()) {
            out.push(' ');
            out.push_str(generic);
        } else {
            out.push(' ');
            out.push('<');
            out.push_str(generic);
            out.push('>');
        }
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
fn format_stmt_kind(out: &mut String, kind: &StmtKind, level: usize) -> bool {
    let nested = stmt_kind_ends_with_newline(kind);
    format_stmt_kind_inner(out, kind, level);
    nested
}

/// Whether a statement kind ends with its own newline (nested-suite
/// kinds and `Self:` blocks render their terminator internally).
fn stmt_kind_ends_with_newline(kind: &StmtKind) -> bool {
    matches!(
        kind,
        StmtKind::Section(_)
            | StmtKind::If { .. }
            | StmtKind::BinderStmt { .. }
            | StmtKind::SelfBlock { .. }
            | StmtKind::FnDecl { suite: Some(_), .. }
    )
}

fn format_stmt_kind_inner(out: &mut String, kind: &StmtKind, level: usize) {
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
        // SURF-0013: `format_stmt` already emitted the single level
        // indent and the trailing newline; this arm must not indent or
        // terminate again (that double-indented equations and left a
        // blank line after every one).
        StmtKind::Equation { left, right } => {
            format_expr(out, left, Prec::Comparison);
            out.push_str(" = ");
            format_expr(out, right, Prec::Comparison);
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

fn is_infer_marker(ty: &TypeExpr) -> bool {
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
        TypeKind::In { base, unit } => {
            format_type(out, base);
            out.push_str(" in ");
            format_type(out, unit);
        }
    }
}
