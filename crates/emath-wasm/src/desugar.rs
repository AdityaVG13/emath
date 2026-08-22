//! Playground-only wrap: a pane that is not already a declaration becomes
//! one. This is not a language change; \`emath-syntax\` / \`emath-sema\` still
//! require an \`emath …:\` header. The wrap lives only in this crate
//! (the pane's engine).

use std::borrow::Cow;

const SYNTH_DECL: &str = "Pane";
const SYNTH_RESULT: &str = "result";

const BUILTINS: &[&str] = &[
    "abs",
    "and",
    "at",
    "atan2",
    "ceil",
    "cos",
    "derivative",
    "else",
    "ensure",
    "exists",
    "exp",
    "false",
    "floor",
    "for",
    "forall",
    "if",
    "in",
    "integral",
    "is_finite",
    "let",
    "ln",
    "log",
    "match",
    "max",
    "min",
    "not",
    "on",
    "or",
    "over",
    "pow",
    "product",
    "require",
    "return",
    "self",
    "sin",
    "sqrt",
    "sum",
    "tan",
    "tanh",
    "then",
    "true",
    "while",
    "with",
    "wrt",
];

/// Source after the playground wrap, plus the visible desugared text
/// when wrapping happened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PreparedSource<'a> {
    pub source: Cow<'a, str>,
    pub is_wrapped: bool,
}

impl<'a> PreparedSource<'a> {
    #[inline]
    #[must_use]
    pub fn desugared(&self) -> Option<&str> {
        if self.is_wrapped {
            Some(self.source.as_ref())
        } else {
            None
        }
    }
}

/// Wrap bare pane text when the first content line is not a declaration header.
#[must_use]
pub(crate) fn prepare_source<'a>(raw: &'a str) -> PreparedSource<'a> {
    if !needs_wrap(raw) {
        return PreparedSource {
            source: Cow::Borrowed(raw),
            is_wrapped: false,
        };
    }
    let wrapped = wrap_bare(raw);
    PreparedSource {
        source: Cow::Owned(wrapped),
        is_wrapped: true,
    }
}

fn needs_wrap(source: &str) -> bool {
    first_content_line(source).is_some_and(|line| !is_declaration_header(line))
}

fn first_content_line(source: &str) -> Option<&str> {
    source
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !is_comment(line))
}

fn is_comment(line: &str) -> bool {
    line.starts_with('#') || line.starts_with("//")
}

fn is_declaration_header(line: &str) -> bool {
    line.strip_prefix("emath")
        .is_some_and(|rest| rest.starts_with(' ') || rest.starts_with('\t'))
}

enum BareLine<'a> {
    Assign { name: &'a str, rhs: &'a str },
    Expr(&'a str),
}

fn wrap_bare(source: &str) -> String {
    let lines = content_lines(source);
    let assigned: Vec<&str> = lines
        .iter()
        .filter_map(|line| match line {
            BareLine::Assign { name, .. } => Some(*name),
            BareLine::Expr(_) => None,
        })
        .collect();
    let mut used: Vec<&str> = Vec::new();
    let mut bound: Vec<&str> = Vec::new();
    for line in &lines {
        let text = match line {
            BareLine::Assign { rhs, .. } => rhs,
            BareLine::Expr(expr) => expr,
        };
        let idents = scan_idents(text);
        for name in binder_names(&idents) {
            if !bound.contains(&name) {
                bound.push(name);
            }
        }
        for ident in idents {
            if !used.contains(&ident) {
                used.push(ident);
            }
        }
    }
    let free: Vec<&str> = used
        .into_iter()
        .filter(|ident| !assigned.contains(ident) && !bound.contains(ident) && !is_builtin(ident))
        .collect();

    let expr_total = lines
        .iter()
        .filter(|line| matches!(line, BareLine::Expr(_)))
        .count();
    let mut expr_index = 0usize;
    let mut defs: Vec<(Cow<'_, str>, &str)> = Vec::with_capacity(lines.len());
    for line in &lines {
        match line {
            BareLine::Assign { name, rhs } => {
                defs.push((Cow::Borrowed(*name), *rhs));
            }
            BareLine::Expr(expr) => {
                expr_index += 1;
                let name = if expr_total <= 1 {
                    Cow::Borrowed(SYNTH_RESULT)
                } else {
                    Cow::Owned(format!("{SYNTH_RESULT}_{expr_index}"))
                };
                defs.push((name, *expr));
            }
        }
    }

    let mut out = String::with_capacity(source.len() + 64);
    out.push_str("emath function ");
    out.push_str(SYNTH_DECL);
    out.push_str(":\n");
    if !free.is_empty() {
        out.push_str("    inputs:\n");
        for name in &free {
            out.push_str("        ");
            out.push_str(name);
            out.push('\n');
        }
        out.push('\n');
    }
    out.push_str("    definitions:\n");
    if defs.is_empty() {
        out.push_str("        result = 0\n");
    } else {
        for (name, rhs) in &defs {
            out.push_str("        ");
            out.push_str(name.as_ref());
            out.push_str(" = ");
            out.push_str(rhs);
            out.push('\n');
        }
    }
    out
}

fn content_lines<'a>(source: &'a str) -> Vec<BareLine<'a>> {
    let mut lines = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || is_comment(trimmed) {
            continue;
        }
        if let Some((lhs, rhs)) = split_assignment(trimmed) {
            let name = lhs.trim();
            if is_ident(name) {
                lines.push(BareLine::Assign {
                    name,
                    rhs: rhs.trim(),
                });
                continue;
            }
        }
        lines.push(BareLine::Expr(trimmed));
    }
    lines
}

fn split_assignment(line: &str) -> Option<(&str, &str)> {
    let bytes = line.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'=' {
            let prev = index.checked_sub(1).and_then(|pos| bytes.get(pos)).copied();
            let next = bytes.get(index + 1).copied();
            if matches!(prev, Some(b'!' | b'<' | b'>' | b'=')) || next == Some(b'=') {
                index += 1;
                continue;
            }
            return Some((&line[..index], &line[index + 1..]));
        }
        index += 1;
    }
    None
}

fn is_ident(text: &str) -> bool {
    let bytes = text.as_bytes();
    let Some(&first) = bytes.first() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == b'_')
        && bytes[1..]
            .iter()
            .all(|&b| b.is_ascii_alphanumeric() || b == b'_')
}

fn is_builtin(name: &str) -> bool {
    BUILTINS.contains(&name)
}

fn is_binder_head(name: &str) -> bool {
    matches!(name, "sum" | "product" | "integral" | "forall" | "exists")
}

/// `sum i in 1..6` binds `i`; it is not a free pane input.
fn binder_names<'a>(idents: &[&'a str]) -> Vec<&'a str> {
    let mut names = Vec::new();
    let mut index = 0;
    while index + 2 < idents.len() {
        if is_binder_head(idents[index]) && idents[index + 2] == "in" {
            names.push(idents[index + 1]);
            index += 3;
            continue;
        }
        index += 1;
    }
    names
}

fn scan_idents<'a>(text: &'a str) -> Vec<&'a str> {
    let bytes = text.as_bytes();
    let mut idents = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        let b = bytes[index];
        if b.is_ascii_alphabetic() || b == b'_' {
            let start = index;
            index += 1;
            while index < bytes.len()
                && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
            {
                index += 1;
            }
            idents.push(&text[start..index]);
            continue;
        }
        if b.is_ascii_digit() {
            index += 1;
            while index < bytes.len()
                && (bytes[index].is_ascii_digit()
                    || bytes[index] == b'.'
                    || bytes[index] == b'e'
                    || bytes[index] == b'E'
                    || bytes[index] == b'+'
                    || bytes[index] == b'-')
            {
                index += 1;
            }
            continue;
        }
        index += 1;
    }
    idents
}
