//! Playground-only wrap: a pane that is not already a declaration becomes
//! one. This is not a language change; `emath-syntax` / `emath-sema` still
//! require an `emath …:` header. The wrap lives only in this crate
//! (the pane's engine).

const SYNTH_DECL: &str = "Pane";
const SYNTH_RESULT: &str = "result";

const BUILTINS: &[&str] = &[
    "abs",
    "atan2",
    "ceil",
    "cos",
    "else",
    "exp",
    "false",
    "floor",
    "if",
    "is_finite",
    "ln",
    "log",
    "max",
    "min",
    "pow",
    "sin",
    "sqrt",
    "tan",
    "tanh",
    "then",
    "true",
];

/// Source after the playground wrap, plus the visible desugared text
/// when wrapping happened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PreparedSource {
    pub source: String,
    pub desugared: Option<String>,
}

/// Wrap bare pane text when the first content line is not a declaration header.
#[must_use]
pub(crate) fn prepare_source(raw: &str) -> PreparedSource {
    if !needs_wrap(raw) {
        return PreparedSource {
            source: raw.to_string(),
            desugared: None,
        };
    }
    let wrapped = wrap_bare(raw);
    PreparedSource {
        source: wrapped.clone(),
        desugared: Some(wrapped),
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

enum BareLine {
    Assign { name: String, rhs: String },
    Expr(String),
}

fn wrap_bare(source: &str) -> String {
    let lines = content_lines(source);
    let assigned: Vec<String> = lines
        .iter()
        .filter_map(|line| match line {
            BareLine::Assign { name, .. } => Some(name.clone()),
            BareLine::Expr(_) => None,
        })
        .collect();
    let mut used = Vec::new();
    for line in &lines {
        let text = match line {
            BareLine::Assign { rhs, .. } => rhs.as_str(),
            BareLine::Expr(expr) => expr.as_str(),
        };
        for ident in scan_idents(text) {
            if !used.iter().any(|existing| existing == &ident) {
                used.push(ident);
            }
        }
    }
    let free: Vec<String> = used
        .into_iter()
        .filter(|ident| !assigned.iter().any(|name| name == ident) && !is_builtin(ident))
        .collect();

    let expr_total = lines
        .iter()
        .filter(|line| matches!(line, BareLine::Expr(_)))
        .count();
    let mut expr_index = 0usize;
    let mut defs = Vec::new();
    for line in &lines {
        match line {
            BareLine::Assign { name, rhs } => {
                defs.push((name.clone(), rhs.clone()));
            }
            BareLine::Expr(expr) => {
                expr_index += 1;
                let name = if expr_total <= 1 {
                    SYNTH_RESULT.to_string()
                } else {
                    format!("{SYNTH_RESULT}_{expr_index}")
                };
                defs.push((name, expr.clone()));
            }
        }
    }

    let mut out = format!("emath function {SYNTH_DECL}:\n");
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
            out.push_str(name);
            out.push_str(" = ");
            out.push_str(rhs);
            out.push('\n');
        }
    }
    out
}

fn content_lines(source: &str) -> Vec<BareLine> {
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
                    name: name.to_string(),
                    rhs: rhs.trim().to_string(),
                });
                continue;
            }
        }
        lines.push(BareLine::Expr(trimmed.to_string()));
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
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn is_builtin(name: &str) -> bool {
    BUILTINS.contains(&name)
}

fn scan_idents(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut idents = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        let ch = chars[index];
        if ch.is_ascii_alphabetic() || ch == '_' {
            let start = index;
            index += 1;
            while index < chars.len()
                && (chars[index].is_ascii_alphanumeric() || chars[index] == '_')
            {
                index += 1;
            }
            idents.push(chars[start..index].iter().collect());
            continue;
        }
        if ch.is_ascii_digit() {
            index += 1;
            while index < chars.len()
                && (chars[index].is_ascii_digit()
                    || chars[index] == '.'
                    || chars[index] == 'e'
                    || chars[index] == 'E'
                    || chars[index] == '+'
                    || chars[index] == '-')
            {
                index += 1;
            }
            continue;
        }
        index += 1;
    }
    idents
}
