//! Printing a closed quoted program as an `emath function` definition
//! (`emath keep`). Structural only: the program's binder, types, and
//! body are printed as written; nothing is simplified or evaluated.
//!
//! Accepted shape: `recur f in A -> B: function x in A: body`. The
//! definition takes `x: A`, returns `result: B`, and every free `f` in
//! the body becomes a call to the definition's own name.

use std::collections::{BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

use super::expr::{path_expr, substitute_path};
use super::identity::mentioned_names;
use super::{CValue, ConstructorError, ExprKind};
use emath_core::tree::Item;
use emath_syntax::formatter::{format_expr, Prec};

fn shape(message: impl Into<String>) -> ConstructorError {
    ConstructorError {
        code: "E-KEEP-SHAPE".into(),
        message: message.into(),
        span: None,
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

fn keep_use(message: impl Into<String>) -> ConstructorError {
    ConstructorError {
        code: "E-KEEP-USE".into(),
        message: message.into(),
        span: None,
    }
}

fn same_file(a: &Path, b: &Path) -> bool {
    let canonical = |p: &Path| {
        p.canonicalize().ok().or_else(|| {
            let parent = p.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
            Some(parent.canonicalize().ok()?.join(p.file_name()?))
        })
    };
    canonical(a).is_some_and(|a| Some(a) == canonical(b))
}

/// The `use` lines the library at `library` needs for `program`: each
/// module the `source` file imports, directly or through its imports,
/// that is the nearest to export a name the program calls, spelled as
/// the library must write it (a shipped module keeps
/// its path; a local file is spelled relative to the library's
/// directory). The library itself is never imported; a local module
/// outside the library's directory refuses `E-KEEP-USE`.
pub fn needed_uses(program: &CValue, source: &Path, library: &Path) -> Result<Vec<String>, ConstructorError> {
    let text = std::fs::read_to_string(source)
        .map_err(|err| keep_use(format!("cannot read {}: {err}", source.display())))?;
    let (tree, diagnostics) = emath_syntax::parse_str(&text);
    if diagnostics.has_errors() {
        return Err(keep_use(format!("cannot parse {}: {diagnostics:?}", source.display())));
    }
    super::prepare_module(&tree, Some(source))?.needed_uses(program, library)
}

impl super::PreparedModule {
    /// Imports for retained Code from this resolved graph, without rereading
    /// source or declarations. The destination determines local path spelling.
    pub fn needed_uses(&self, program: &CValue, library: &Path) -> Result<Vec<String>, ConstructorError> {
        needed_uses_from_graph(self, program, library)
    }
}

fn needed_uses_from_graph(
    prepared: &super::PreparedModule,
    program: &CValue,
    library: &Path,
) -> Result<Vec<String>, ConstructorError> {
    let CValue::Code(code) = program else {
        return Err(shape("the kept value is not Code"));
    };
    let mut mentioned = BTreeSet::new();
    mentioned_names(&code.expr, &mut mentioned);
    let library_dir = library.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
    // Breadth first from the source's own imports: a program built
    // inside an imported module can call what that module imports, and
    // the nearest module exporting a name is the one in scope.
    let mut queue: VecDeque<_> = prepared.imports.root_uses.iter().collect();
    let mut seen: Vec<PathBuf> = Vec::new();
    let mut covered: BTreeSet<String> = BTreeSet::new();
    let mut uses = Vec::new();
    while let Some(import) = queue.pop_front() {
        let path = &import.path;
        let file = &import.file;
        if same_file(file, library) || seen.iter().any(|s| same_file(s, file)) {
            continue;
        }
        seen.push(file.clone());
        let module = prepared.imports.module(file);
        queue.extend(&module.uses);
        let exports: Vec<&String> = module.tree
            .items
            .iter()
            .flat_map(|item| match item {
                Item::Exports { names, .. } => names.iter().collect(),
                _ => Vec::new(),
            })
            .collect();
        // Private and clashing names reach a program as `{name}__{label}`
        // (scope.rs `key`), with label the module path joined by `_` and
        // maybe a `_N` suffix. No library can spell them.
        let label = path.join("_");
        for key in &mentioned {
            let Some((name, tag)) = key.rsplit_once("__") else { continue };
            let labelled = tag == label
                || tag.strip_prefix(label.as_str()).and_then(|rest| rest.strip_prefix('_')).is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()));
            let declared = module.tree.items.iter().any(|item| matches!(item, Item::Declaration(decl) if decl.name == name));
            if labelled && declared {
                let module_name = path.join(".");
                return Err(keep_use(if exports.iter().any(|e| e.as_str() == name) {
                    format!("the program calls `{name}`, which more than one imported module exports; a kept program must name exactly one")
                } else {
                    format!("the program calls `{name}`, which is private to `{module_name}`; export it from `{module_name}` to keep this program")
                }));
            }
        }
        let provides: Vec<String> = exports
            .iter()
            .filter(|name| mentioned.contains(name.as_str()) && !covered.contains(name.as_str()))
            .map(|name| name.to_string())
            .collect();
        if provides.is_empty() {
            continue;
        }
        covered.extend(provides);
        let spelled = if let Some(relative) = prepared.roots.iter().find_map(|root| file.strip_prefix(root).ok()) {
            relative.with_extension("").iter().map(|part| part.to_string_lossy()).collect::<Vec<_>>().join(".")
        } else {
            let (Ok(file), Ok(dir)) = (file.canonicalize(), library_dir.canonicalize()) else {
                return Err(keep_use(format!("cannot locate `{}` for the library", path.join("."))));
            };
            let rel = file.strip_prefix(&dir).map_err(|_| {
                keep_use(format!(
                    "`use {}` resolves to {}, which the library at {} cannot import: keep the library beside or above it",
                    path.join("."),
                    file.display(),
                    library.display()
                ))
            })?;
            rel.with_extension("").iter().map(|part| part.to_string_lossy()).collect::<Vec<_>>().join(".")
        };
        if !uses.contains(&spelled) {
            uses.push(spelled);
        }
    }
    Ok(uses)
}

/// `text` with a `use` line for each of `uses` it lacks, after its last
/// `use` line (or after the `exports:` entry).
fn with_uses(text: &str, uses: &[String]) -> String {
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    for module in uses {
        let line = format!("use {module}");
        if lines.iter().any(|l| l.trim_end() == line) {
            continue;
        }
        let at = match lines.iter().rposition(|l| l.starts_with("use ")) {
            Some(last) => last + 1,
            None => {
                let Some(exports) = lines.iter().position(|l| l.starts_with("exports:")) else {
                    continue;
                };
                let mut end = exports + 1;
                while end < lines.len() && lines[end].starts_with(char::is_whitespace) && !lines[end].trim().is_empty() {
                    end += 1;
                }
                lines.insert(end, String::new());
                end + 1
            }
        };
        lines.insert(at, line);
    }
    lines.join("\n") + "\n"
}

/// Keep `program` under `name` in a kept library's text (`None`: no
/// library yet), adding the `use` lines it needs (`needed_uses`).
/// Returns what happened and the content id of the definition (its
/// text without the summary line). The summary line reads `Kept from
/// {origin}; content {id}.`; the same content again (with every use
/// present) is `Unchanged`; different content under a kept name
/// refuses `E-KEEP-EXISTS` unless `overwrite`; a text with no
/// `exports:` line refuses `E-KEEP-LIBRARY`.
pub fn place_definition(
    library: Option<&str>,
    name: &str,
    program: &CValue,
    origin: &str,
    overwrite: bool,
    uses: &[String],
) -> Result<(Placed, String), ConstructorError> {
    let content = emath_core::content_id_of_str(&kept_definition(program, name, "")?).0;
    let definition = kept_definition(program, name, &format!("Kept from {origin}; content {content}."))?;
    let Some(text) = library else {
        let fresh = format!("# Kept definitions, written by `emath keep`.\nexports: {name}\n\n{definition}");
        return Ok((Placed::Kept(with_uses(&fresh, uses)), content));
    };
    let used = with_uses(text, uses);
    let text = used.as_str();
    let lines: Vec<&str> = text.lines().collect();
    let updated = match definition_lines(&lines, name) {
        Some((start, end)) => {
            if lines[start..end].join("\n").contains(&format!("content {content}.")) {
                let original = library.unwrap_or_default();
                if used.trim_end() == original.trim_end() {
                    return Ok((Placed::Unchanged, content));
                }
                return Ok((Placed::Kept(used.clone()), content));
            }
            if !overwrite {
                return Err(ConstructorError {
                    code: "E-KEEP-EXISTS".into(),
                    message: format!("the library already defines `{name}` with different content"),
                    span: None,
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
                    span: None,
                })
            }
        },
    };
    Ok((Placed::Kept(updated), content))
}

/// One kept item: its name, its program, and an optional note the
/// library's summary line carries (the item's `note` field, rendered as
/// text: a `Str` as written, any other value as it prints).
#[derive(Clone, Debug)]
pub struct NamedProgram {
    pub name: String,
    pub program: CValue,
    pub note: Option<String>,
}

/// The kept items of a value that names its own programs: a sequence of
/// records each with a `name` field holding a quoted identifier
/// (`quote(triangular)`), a `program` field holding Code, and an
/// optional `note`. Other fields are ignored; the order is kept.
pub fn named_programs(value: &CValue) -> Result<Vec<NamedProgram>, ConstructorError> {
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
            let note = fields.get("note").map(|note| match note {
                CValue::Str(text) => text.clone(),
                other => other.to_string(),
            });
            Ok(NamedProgram { name, program, note })
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
