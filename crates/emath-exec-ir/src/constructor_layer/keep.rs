//! Printing a closed quoted program as an `emath function` definition
//! (`emath keep`). Structural only: the program's binder, types, and
//! body are printed as written; nothing is simplified or evaluated.
//!
//! Accepted shape: `recur f in A -> B: function x in A: body`. The
//! definition takes `x: A`, returns `result: B`, and every free `f` in
//! the body becomes a call to the definition's own name.

use super::expr::{path_expr, substitute_path};
use super::{CValue, ConstructorError, ExprKind};
use emath_syntax::formatter::{format_expr, Prec};

fn shape(message: impl Into<String>) -> ConstructorError {
    ConstructorError {
        code: "E-KEEP-SHAPE".into(),
        message: message.into(),
    }
}

fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// What keeping one program into a library text did.
pub enum Placed {
    /// The library already holds this content under the name.
    Unchanged,
    /// The new library text.
    Kept(String),
}

/// The line range `[start, end)` of `emath function {name}` in a
/// library: from its head to the next top-level declaration, trailing
/// blank lines excluded.
fn definition_lines(lines: &[&str], name: &str) -> Option<(usize, usize)> {
    let head = format!("emath function {name}:");
    let start = lines.iter().position(|line| line.trim_end() == head)?;
    let mut end = lines[start + 1..]
        .iter()
        .position(|line| line.starts_with("emath "))
        .map_or(lines.len(), |k| start + 1 + k);
    while end > start + 1 && lines[end - 1].trim().is_empty() {
        end -= 1;
    }
    Some((start, end))
}

fn with_export(text: &str, name: &str) -> Option<String> {
    let mut found = false;
    let lines: Vec<String> = text
        .lines()
        .map(|line| match line.strip_prefix("exports:") {
            Some(rest) if !found => {
                found = true;
                let names: Vec<&str> = rest.split(',').map(str::trim).filter(|n| !n.is_empty()).collect();
                if names.contains(&name) {
                    line.to_string()
                } else {
                    format!("exports: {}", names.into_iter().chain([name]).collect::<Vec<_>>().join(", "))
                }
            }
            _ => line.to_string(),
        })
        .collect();
    found.then(|| lines.join("\n") + "\n")
}

/// Keep `program` under `name` in a kept library's text (`None`: no
/// library yet). Returns what happened and the content id of the
/// definition (its text without the summary line). The summary line
/// reads `Kept from {origin}; content {id}.`; the same content again
/// is `Unchanged`; different content under a kept name refuses
/// `E-KEEP-EXISTS` unless `overwrite`; a text with no `exports:` line
/// refuses `E-KEEP-LIBRARY`.
pub fn place_definition(
    library: Option<&str>,
    name: &str,
    program: &CValue,
    origin: &str,
    overwrite: bool,
) -> Result<(Placed, String), ConstructorError> {
    let content = emath_core::content_id_of_str(&kept_definition(program, name, "")?).0;
    let definition = kept_definition(program, name, &format!("Kept from {origin}; content {content}."))?;
    let Some(text) = library else {
        let fresh = format!("# Kept definitions, written by `emath keep`.\nexports: {name}\n\n{definition}");
        return Ok((Placed::Kept(fresh), content));
    };
    let lines: Vec<&str> = text.lines().collect();
    let updated = match definition_lines(&lines, name) {
        Some((start, end)) => {
            if lines[start..end].join("\n").contains(&format!("content {content}.")) {
                return Ok((Placed::Unchanged, content));
            }
            if !overwrite {
                return Err(ConstructorError {
                    code: "E-KEEP-EXISTS".into(),
                    message: format!("the library already defines `{name}` with different content"),
                });
            }
            let mut out: Vec<&str> = lines[..start].to_vec();
            out.extend(definition.lines());
            out.extend(&lines[end..]);
            out.join("\n") + "\n"
        }
        None => match with_export(text, name) {
            Some(exported) => format!("{}\n\n{definition}", exported.trim_end_matches('\n')),
            None => {
                return Err(ConstructorError {
                    code: "E-KEEP-LIBRARY".into(),
                    message: "the file has no `exports:` line; it is not a kept library".into(),
                })
            }
        },
    };
    Ok((Placed::Kept(updated), content))
}

/// The (name, program) pairs of a value that names its own programs: a
/// sequence of records each with a `name` field holding a quoted
/// identifier (`quote(triangular)`) and a `program` field holding Code.
/// Other fields are ignored; the order is kept.
pub fn named_programs(value: &CValue) -> Result<Vec<(String, CValue)>, ConstructorError> {
    let CValue::Sequence(items) = value else {
        return Err(shape("without a name, the kept value must be a sequence of {name, program} records"));
    };
    items
        .iter()
        .map(|item| {
            let CValue::Record { fields, .. } = item else {
                return Err(shape("each kept item must be a record with `name` and `program`"));
            };
            let name = match fields.get("name") {
                Some(CValue::Code(code)) => match &code.expr.kind {
                    ExprKind::Path { segments, .. } if segments.len() == 1 => segments[0].clone(),
                    _ => return Err(shape("a kept `name` must be a quoted identifier such as quote(triangular)")),
                },
                _ => return Err(shape("a kept item has no quoted `name`")),
            };
            let program = fields
                .get("program")
                .cloned()
                .ok_or_else(|| shape(format!("the kept item `{name}` has no `program`")))?;
            Ok((name, program))
        })
        .collect()
}

/// The source text of `emath function {name}` for a kept program.
/// `note` becomes the definition's summary comment.
pub fn kept_definition(value: &CValue, name: &str, note: &str) -> Result<String, ConstructorError> {
    if !is_identifier(name) {
        return Err(shape(format!("`{name}` is not a definition name")));
    }
    let CValue::Code(code) = value else {
        return Err(shape("the kept value is not Code"));
    };
    let ExprKind::Recur { name: self_name, ty, body } = &code.expr.kind else {
        return Err(shape("expected `recur f in A -> B: function x in A: body`"));
    };
    let ExprKind::FunctionAbs { param, domain, body } = &body.kind else {
        return Err(shape("the recur body is not `function x in A: ...`"));
    };
    let output = match &ty.kind {
        ExprKind::Call { function, args }
            if args.len() == 2
                && matches!(&function.kind, ExprKind::Path { segments, .. } if segments.len() == 1 && segments[0] == "Fn") =>
        {
            &args[1]
        }
        _ => return Err(shape("the recur type is not `A -> B`")),
    };
    if param == name || param == "result" {
        return Err(shape(format!("the parameter `{param}` collides with `{name}` or `result`")));
    }
    let mut mint = 0;
    let body = substitute_path(body, self_name, &path_expr(name), &mut mint);
    let (mut dom, mut out, mut text) = (String::new(), String::new(), String::new());
    format_expr(&mut dom, domain, Prec::Root);
    format_expr(&mut out, output, Prec::Root);
    format_expr(&mut text, &body, Prec::Root);
    Ok(format!(
        "emath function {name}:\n    # {note}\n    inputs:\n        {param}: {dom}\n    outputs:\n        result: {out}\n    definitions:\n        result = {text}\n"
    ))
}
