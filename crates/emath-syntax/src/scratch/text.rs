//! Line classification and top-level text splitting helpers.

use super::*;

#[derive(Clone, Debug)]
pub(super) enum LineKind {
    Assign { name: String, rhs: String },
    Example { name: String, value: String },
    Expr(String),
    Intent { verb: IntentVerb, payload: String },
    Hole { name: String },
    Require { expr: String },
    Invalid,
}

pub(super) fn classify_line(line: &str) -> LineKind {
    let trimmed = line.trim();
    if let Some(rest) = trimmed.strip_prefix("require ") {
        return LineKind::Require {
            expr: rest.trim().to_string(),
        };
    }
    if let Some((lhs, rhs)) = split_assignment(trimmed) {
        if rhs.trim() == "?" {
            let head = lhs.trim();
            let name = head.split('(').next().unwrap_or(head).trim().to_string();
            if is_ident(&name) {
                return LineKind::Hole { name };
            }
        }
    }
    if let Some(rest) = trimmed.strip_prefix("example ") {
        if let Some((name, value)) = split_assignment(rest) {
            let name = name.trim();
            if is_ident(name) {
                return LineKind::Example {
                    name: name.to_string(),
                    value: value.trim().to_string(),
                };
            }
        }
        return LineKind::Invalid;
    }
    let first = first_word(trimmed);
    if let Some(verb) = IntentVerb::parse_word(first) {
        let payload = trimmed[first.len()..].trim_start();
        return LineKind::Intent {
            verb,
            payload: payload.to_string(),
        };
    }
    if let Some((lhs, rhs)) = split_assignment(trimmed) {
        let name = lhs.trim();
        if is_ident(name) {
            return LineKind::Assign {
                name: name.to_string(),
                rhs: rhs.trim().to_string(),
            };
        }
    }
    if looks_like_expression(trimmed) {
        return LineKind::Expr(trimmed.to_string());
    }
    LineKind::Invalid
}

pub(super) fn looks_like_expression(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }
    if is_ident(trimmed) || is_number(trimmed) {
        return true;
    }
    trimmed.chars().any(|ch| {
        matches!(
            ch,
            '+' | '-' | '*' | '/' | '^' | '(' | ')' | '[' | ']' | ',' | '.' | '<' | '>' | '=' | '!'
        )
    })
}

pub(super) fn is_number(text: &str) -> bool {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_digit()
        || (first == '.' && chars.next().is_some_and(|c| c.is_ascii_digit()))
        || (first == '-'
            && text[1..]
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_digit() || c == '.'))
}

pub(super) fn goal_target(payload: &str) -> String {
    let mut out = String::new();
    for ch in payload.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    let slug = out.trim_matches('_').to_string();
    if slug.is_empty() {
        "result".to_string()
    } else {
        slug
    }
}

pub(super) fn first_word(line: &str) -> &str {
    line.split(|c: char| c.is_whitespace())
        .next()
        .unwrap_or(line)
}

pub(super) fn split_keyword_tail<'a>(
    payload: &'a str,
    keyword: &str,
) -> (&'a str, Option<&'a str>) {
    let needle = format!(" {keyword} ");
    if let Some(index) = payload.rfind(&needle) {
        let expr = &payload[..index];
        let tail = payload[index + needle.len()..].trim();
        (expr, Some(tail))
    } else if let Some(rest) = payload.strip_suffix(&format!(" {keyword}")) {
        (rest, None)
    } else {
        (payload, None)
    }
}

pub(super) fn split_equation(equation: &str) -> (&str, &str) {
    if let Some(index) = equation.find("==") {
        return (equation[..index].trim(), equation[index + 2..].trim());
    }
    if let Some((lhs, rhs)) = split_assignment(equation) {
        return (lhs.trim(), rhs.trim());
    }
    (equation, "0")
}

pub(super) fn literal_class(value: &str) -> &'static str {
    let value = value.trim();
    if value == "true" || value == "false" {
        "bool"
    } else if is_number(value) {
        "number"
    } else if value.starts_with('"') || value.starts_with('\'') {
        "string"
    } else {
        "other"
    }
}

pub(super) fn is_item_header(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with('@')
        || is_emath_keyword_prefix(trimmed)
        || trimmed.starts_with("package ")
        || trimmed.starts_with("use ")
        || trimmed.starts_with("notation ")
        || trimmed.starts_with("extern ")
}

pub(super) fn is_section_head(line: &str) -> bool {
    let trimmed = line.trim();
    let Some(name) = trimmed.split(':').next() else {
        return false;
    };
    let name = name.trim();
    SECTION_HEADS.contains(&name) && trimmed[name.len()..].trim_start().starts_with(':')
}

pub(super) fn is_comment(line: &str) -> bool {
    line.starts_with('#') || line.starts_with("//")
}

pub(super) fn is_content_line(s: &str) -> bool {
    !s.is_empty() && !is_comment(s)
}

pub(super) fn first_content_line(source: &str) -> Option<&str> {
    source
        .lines()
        .map(str::trim)
        .find(|line| is_content_line(line))
}

pub(super) fn line_offsets(source: &str) -> Vec<(usize, &str)> {
    let mut offset = 0usize;
    let mut out = Vec::new();
    for line in source.split_inclusive('\n') {
        let text = line.strip_suffix('\n').unwrap_or(line);
        out.push((offset, text));
        offset += line.len();
    }
    out
}

pub(super) fn span_of_source(source: &str) -> Span {
    span_bytes(0, source.len())
}

pub(super) fn span_bytes(start: usize, len: usize) -> Span {
    let start = u32::try_from(start).unwrap_or(u32::MAX);
    let end = start.saturating_add(u32::try_from(len).unwrap_or(u32::MAX));
    Span::new(FileId(0), start, end)
}

pub(super) enum TopPiece {
    Declaration(String),
    Other(String),
}

pub(super) fn is_unindented(line: &str) -> bool {
    !line.starts_with(' ') && !line.starts_with('\t')
}

pub(super) fn is_emath_keyword_prefix(s: &str) -> bool {
    s.starts_with("emath ") || s.starts_with("emath\t")
}

pub(super) fn split_top_level(source: &str) -> Vec<TopPiece> {
    let mut pieces = Vec::new();
    let mut current = String::new();
    let mut current_decl = false;
    let mut started = false;
    for line in source.split_inclusive('\n') {
        let trimmed = line.trim_start();
        let at_margin =
            is_emath_keyword_prefix(line) || trimmed.starts_with("emath ") && is_unindented(line);
        let margin_header =
            is_unindented(line) && (is_emath_keyword_prefix(trimmed) || trimmed.starts_with('@'));
        if started && margin_header && at_margin {
            pieces.push(if current_decl {
                TopPiece::Declaration(std::mem::take(&mut current))
            } else {
                TopPiece::Other(std::mem::take(&mut current))
            });
            current_decl = true;
            started = true;
            current.push_str(line);
            continue;
        }
        if !started {
            current_decl = margin_header && is_emath_keyword_prefix(trimmed);
            started = true;
        }
        current.push_str(line);
    }
    if started || !current.is_empty() {
        pieces.push(if current_decl {
            TopPiece::Declaration(current)
        } else {
            TopPiece::Other(current)
        });
    }
    pieces
}

pub(super) fn split_declaration_text(text: &str) -> Option<(String, Option<String>, String)> {
    let mut header = String::new();
    let mut rest_start = None;
    for (index, line) in text.split_inclusive('\n').enumerate() {
        let trimmed = line.trim_start();
        if index == 0
            || (header.trim_end().ends_with(',')
                || !header.contains(':') && trimmed.starts_with('@'))
        {
            header.push_str(line);
            continue;
        }
        rest_start = Some(index);
        break;
    }
    let header_line = header.lines().last()?.trim();
    let (head, inline) = split_header_colon(header_line)?;
    let body = if rest_start.is_some() {
        text.split_inclusive('\n').skip(1).collect()
    } else {
        String::new()
    };
    let lines: Vec<&str> = header.lines().collect();
    let mut prefix = lines[..lines.len().saturating_sub(1)].join("\n");
    if lines.len() > 1 {
        prefix.push('\n');
    }
    prefix.push_str(&head);
    prefix.push(':');
    Some((prefix, inline, body))
}

pub(super) fn split_header_colon(header: &str) -> Option<(String, Option<String>)> {
    let bytes = header.as_bytes();
    let mut i = 0;
    // skip `emath`
    i = skip_ws(bytes, i);
    i = skip_word(bytes, i);
    i = skip_ws(bytes, i);
    i = skip_word(bytes, i); // kind
    i = skip_ws(bytes, i);
    i = skip_word(bytes, i); // name
    i = skip_ws(bytes, i);
    if i < bytes.len() && bytes[i] == b'<' {
        i = skip_balanced(bytes, i, b'<', b'>')?;
    }
    i = skip_ws(bytes, i);
    if i < bytes.len() && bytes[i] == b'(' {
        i = skip_balanced(bytes, i, b'(', b')')?;
    }
    i = skip_ws(bytes, i);
    if i + 1 < bytes.len() && bytes[i] == b'-' && bytes[i + 1] == b'>' {
        i += 2;
        i = skip_ws(bytes, i);
        i = skip_word(bytes, i);
    }
    i = skip_ws(bytes, i);
    if i >= bytes.len() || bytes[i] != b':' {
        return None;
    }
    let head = header[..i].trim_end().to_string();
    let inline = header[i + 1..].trim();
    let inline = if inline.is_empty() {
        None
    } else {
        Some(inline.to_string())
    };
    Some((head, inline))
}

pub(super) fn skip_ws(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    i
}

pub(super) fn skip_word(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
        i += 1;
    }
    i
}

pub(super) fn skip_balanced(bytes: &[u8], mut i: usize, open: u8, close: u8) -> Option<usize> {
    if i >= bytes.len() || bytes[i] != open {
        return None;
    }
    let mut depth = 0i32;
    while i < bytes.len() {
        if bytes[i] == open {
            depth += 1;
        } else if bytes[i] == close {
            depth -= 1;
            if depth == 0 {
                return Some(i + 1);
            }
        }
        i += 1;
    }
    None
}

pub(super) fn header_args(header: &str) -> Vec<(String, Option<String>)> {
    let bytes = header.as_bytes();
    let Some(start) = header.find('(') else {
        return Vec::new();
    };
    let Some(end) = skip_balanced(bytes, start, b'(', b')') else {
        return Vec::new();
    };
    let inner = header[start + 1..end - 1].trim();
    if inner.is_empty() {
        return Vec::new();
    }
    let mut args = Vec::new();
    for part in inner.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let (name, ty) = match part.split_once(':') {
            Some((name, ty)) => (name.trim(), Some(ty.trim().to_string())),
            None => (part, None),
        };
        if !name.is_empty() {
            args.push((name.to_string(), ty));
        }
    }
    args
}

pub(super) fn call_position_names(text: &str) -> Vec<String> {
    let bytes = text.as_bytes();
    let mut names = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index].is_ascii_alphabetic() || bytes[index] == b'_' {
            let start = index;
            index = skip_word(bytes, index);
            let name = &text[start..index];
            let next = skip_ws(bytes, index);
            if next < bytes.len()
                && bytes[next] == b'('
                && is_ident(name)
                && !names.iter().any(|existing| existing == name)
            {
                names.push(name.to_string());
            }
            continue;
        }
        index += 1;
    }
    names
}

pub(super) fn declaration_name(header: &str) -> Option<&str> {
    let rest = header.trim().strip_prefix("emath")?.trim_start();
    let rest = rest.split_whitespace().nth(1)?;
    let name = rest
        .split('(')
        .next()?
        .split('<')
        .next()?
        .trim_end_matches(':');
    Some(name)
}
