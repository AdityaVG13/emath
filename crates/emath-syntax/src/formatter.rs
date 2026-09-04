//! Canonical formatter.
//!
//! Idempotent, comment-preserving printer over the `SyntaxTree`: canonical
//! layout, parentheses re-introduced from the precedence table so
//! `parse(format(x))` == `parse(x)`. Comments retained by span order.

use crate::token::Comment;
use crate::tree::{
    Argument, ArgumentValue, Attribute, Binder, BinderKind, CommandArgument, Declaration,
    GenericArg, Item, NotationDecl, NotationFixity, Param, Place, ReactionArrow, ReactionTerm,
    Section, Stmt, StmtKind, Suite, SyntaxTree, TypeExpr, TypeKind, UseTree, Visibility,
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

impl Prec {
    /// Next tighter level. Used so equal-precedence children keep the
    /// parentheses the parser needs for left vs right associativity.
    #[must_use]
    pub(super) const fn tighter(self) -> Self {
        match self {
            Self::Root => Self::Iff,
            Self::Iff => Self::Imply,
            Self::Imply => Self::Or,
            Self::Or => Self::And,
            Self::And => Self::Comparison,
            Self::Comparison => Self::Additive,
            Self::Additive => Self::Multiplicative,
            Self::Multiplicative => Self::Power,
            Self::Power => Self::Unary,
            Self::Unary | Self::Atomic => Self::Atomic,
        }
    }
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

pub(super) fn push_string_literal(out: &mut String, text: &str) {
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
    // Attributes render before the head, matching
    // `emath_item = { attribute }, "emath", ...`.
    for attribute in &decl.attributes {
        format_attribute(out, attribute, level);
    }
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
    // Grammar spelling: `@path(...)` — the attribute prefix is `@`, not
    // a Rust-style `#[...]` bracket.
    out.push('@');
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
    out.push('\n');
}

/// Section names whose parser dispatch requires the bare two-word head
/// (the rest render generics in angle brackets, re-parsed identically).
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
    // 04 §5.3: the generic fit goal renders
    // `fit <params> to <observable>:` — parameters in args (Expr path
    // arguments), observable in the generic slot — never the
    // angle-bracket generic spelling.
    if section.name == "fit" {
        out.push_str("fit");
        if let Some(args) = &section.args {
            out.push(' ');
            for (i, arg) in args.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                format_argument(out, arg);
            }
        }
        if let Some(generic) = &section.generic {
            out.push_str(" to ");
            out.push_str(generic);
        }
        out.push(':');
        return;
    }
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

/// `observations:` rows (04 §5.2): every row
/// is an `obs`-prefixed `FieldDecl`; render the prefix and keep the
/// statement body byte-identical to the plain field spelling.
fn format_observations_suite(out: &mut String, suite: &Suite, level: usize) {
    for stmt in &suite.statements {
        indent(out, level);
        if matches!(stmt.kind, StmtKind::FieldDecl { .. }) {
            out.push_str("obs ");
        }
        if !format_stmt_kind(out, &stmt.kind, level) {
            out.push('\n');
        }
    }
}

mod stmt;
use stmt::*;
pub use stmt::{format_generic_arg, format_type};
