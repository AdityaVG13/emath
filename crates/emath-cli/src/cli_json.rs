//! Diagnostic printing and JSON document helpers shared by CLI commands.

use super::{Diagnostics, CliExit};

/// The file a diagnostic points into, when `store` holds it. A diagnostic
/// with the empty default span has no position.
fn located<'a>(item: &emath_core::Diagnostic, store: &'a emath_core::SourceStore) -> Option<&'a emath_core::SourceFile> {
    if (item.primary.start, item.primary.end) == (0, 0) {
        return None;
    }
    store.get(item.primary.file)
}

/// The main file as a line map, for diagnostics that point into it.
pub fn source_lines(name: &str, source: &str) -> emath_core::SourceStore {
    let mut store = emath_core::SourceStore::new();
    let _ = store.try_add(name, source);
    store
}

/// Print diagnostics as `path:line:col: CODE: message` with the source
/// line and a caret, when they point into `source`. Only for diagnostics
/// from `parse_str` or a fresh session, whose main file is `FileId(0)`.
pub fn print_diagnostics_in(diagnostics: &Diagnostics, name: &str, source: &str) {
    print_diagnostics_from(diagnostics, &source_lines(name, source));
}

/// Print diagnostics located against the files a session loaded.
pub fn print_diagnostics_from(diagnostics: &Diagnostics, store: &emath_core::SourceStore) {
    eprintln!("{}", diagnostics_text(diagnostics, store));
}

pub(crate) fn diagnostics_text(diagnostics: &Diagnostics, store: &emath_core::SourceStore) -> String {
    let mut lines = Vec::new();
    for item in diagnostics.items() {
        if located(item, store).is_some() {
            lines.push(store.render(item));
            continue;
        }
        lines.push(format!("{} {}", item.code, item.message));
        if let Some(help) = &item.help {
            lines.extend(help.lines().map(|line| format!("  {line}")));
        }
    }
    lines.join("\n")
}

/// Why a program was refused before it ran: the first error's code and
/// message, and the full located text for stderr.
pub struct Refusal {
    pub code: String,
    pub message: String,
    pub text: String,
}

/// Parse and admit a constructor program the way `check` does, so every
/// command that runs or translates a program refuses a broken one the
/// same way (exit 2, located).
pub fn admitted_tree(path: &std::path::Path, source: &str) -> Result<emath_syntax::tree::SyntaxTree, Refusal> {
    let name = path.display().to_string();
    let checked = crate::cli_build::checked_text(&name, source);
    if let Some(first) = checked.result.diagnostics
        .items()
        .iter()
        .find(|item| item.severity == emath_core::Severity::Error)
    {
        return Err(Refusal {
            code: first.code.to_string(),
            message: first.message.clone(),
            text: diagnostics_text(&checked.result.diagnostics, &checked.store),
        });
    }
    checked.result.prepared.map(|module| module.tree().clone()).ok_or_else(|| Refusal {
        code: "E-PKG-081".into(),
        message: "checked source has no constructor module".into(),
        text: "E-PKG-081 checked source has no constructor module".into(),
    })
}

/// One error line for stderr: `path:line:col: code: message` with the
/// source line and a caret when `span` points into `source`, else
/// `error: code: message`.
pub fn error_text(code: &str, message: &str, name: &str, source: &str, span: Option<emath_core::Span>) -> String {
    let store = source_lines(name, source);
    let (Some(span), Some(file)) = (span, store.get(emath_core::FileId(0))) else {
        return format!("error: {code}: {message}");
    };
    if (span.start, span.end) == (0, 0) {
        return format!("error: {code}: {message}");
    }
    let (line, column) = file.line_col(span.start);
    let text = file.line_text(line.saturating_sub(1) as usize);
    let indent = text.get(..column.saturating_sub(1) as usize).map_or(0, |prefix| prefix.chars().count());
    format!("{name}:{line}:{column}: {code}: {message}\n  {text}\n  {}^", " ".repeat(indent))
}

/// 1-based line and column of a diagnostic in the main file, when known.
fn line_column(item: &emath_core::Diagnostic, store: Option<&emath_core::SourceStore>) -> Option<(u32, u32)> {
    located(item, store?).map(|file| file.line_col(item.primary.start))
}

/// One `{code,severity,message}` diagnostic object for `--json` envelopes.
pub fn json_diagnostic_entry(code: &str, severity: &str, message: &str) -> String {
    let mut entry = emath_artifact::JsonWriter::object();
    entry.string("code", code);
    entry.string("severity", severity);
    entry.string("message", message);
    entry.finish().trim_end().to_string()
}

pub fn json_put_opt(entry: &mut emath_artifact::JsonObject, key: &str, value: Option<&str>) {
    if let Some(value) = value {
        entry.string(key, value);
    }
}

pub fn json_diagnostics_entries(diagnostics: &Diagnostics) -> Vec<String> {
    json_diagnostics_entries_in(diagnostics, None)
}

/// As [`json_diagnostics_entries`], adding `line` and `column` (1-based)
/// for diagnostics that point into the main file of `store`.
pub fn json_diagnostics_entries_in(diagnostics: &Diagnostics, store: Option<&emath_core::SourceStore>) -> Vec<String> {
    diagnostics
        .items()
        .iter()
        .map(|item| {
            let mut entry = emath_artifact::JsonWriter::object();
            entry.string("code", item.code);
            if let Some((line, column)) = line_column(item, store) {
                entry.int("line", u64::from(line));
                entry.int("column", u64::from(column));
            }
            entry.string(
                "severity",
                match item.severity {
                    emath_core::Severity::Error => "error",
                    emath_core::Severity::Warning => "warning",
                    emath_core::Severity::Note => "note",
                },
            );
            entry.string("message", &item.message);
            json_put_opt(&mut entry, "help", item.help.as_deref());
            if let Some(pedagogy) = &item.pedagogy {
                entry.string("understood", &pedagogy.understood);
                entry.string("unknown", &pedagogy.unknown);
                entry.string("why", &pedagogy.why);
                entry.string("smallest_repair", &pedagogy.smallest_repair);
                if !pedagogy.alternatives.is_empty() {
                    entry.strings("alternatives", &pedagogy.alternatives);
                }
                json_put_opt(&mut entry, "example", pedagogy.example.as_deref());
                json_put_opt(
                    &mut entry,
                    "deeper_concept",
                    pedagogy.deeper_concept.as_deref(),
                );
                json_put_opt(
                    &mut entry,
                    "authority_consequence",
                    pedagogy.authority_consequence.as_deref(),
                );
                json_put_opt(&mut entry, "library_link", pedagogy.library_link.as_deref());
            }
            entry.finish().trim_end().to_string()
        })
        .collect()
}

/// Stdout envelope for `--json` command refusals (`check`/`eval` pattern).
/// Uniform contract: every envelope carries `command`, a `status` string
/// (`ok`|`refused`), and `diagnostics`. Deterministic, no timestamps.
pub fn diagnostics_json_document(command: &str, admitted: bool, entries: &[String]) -> String {
    let mut out = emath_artifact::JsonWriter::object();
    out.string("command", command);
    out.string("status", if admitted { "ok" } else { "refused" });
    out.bool("admitted", admitted);
    out.objects("diagnostics", entries);
    out.finish()
}

pub fn print_json_diagnostics(command: &str, admitted: bool, entries: &[String]) {
    println!("{}", diagnostics_json_document(command, admitted, entries));
}

pub fn refuse_coded(
    command: &str,
    json: bool,
    exit: CliExit,
    code: &str,
    message: &str,
) -> CliExit {
    eprintln!("error: {code}: {message}");
    if json {
        print_json_diagnostics(
            command,
            false,
            &[json_diagnostic_entry(code, "error", message)],
        );
    }
    exit
}

/// Stdout envelope for `emath check --json`.
pub fn check_json_document(
    admitted: bool,
    package_id: &str,
    diagnostics: &Diagnostics,
    meaning_id: Option<&str>,
    units_profiles: &[(String, String)],
    source: Option<&emath_core::SourceStore>,
) -> String {
    let mut out = emath_artifact::JsonWriter::object();
    out.string("schema_version", "emath.constructor.v1");
    out.string("command", "check");
    out.string("status", if admitted { "ok" } else { "refused" });
    out.string("admission", if admitted { "admitted" } else { "refused" });
    out.bool("admitted", admitted);
    out.objects("diagnostics", &json_diagnostics_entries_in(diagnostics, source));
    out.string("package", package_id);
    json_put_opt(&mut out, "meaning_id", meaning_id);
    let rows: Vec<String> = units_profiles
        .iter()
        .map(|(declaration, profile)| {
            let mut row = emath_artifact::JsonWriter::object();
            row.string("declaration", declaration);
            row.string("profile", profile);
            row.finish().trim_end().to_string()
        })
        .collect();
    out.objects("units_profiles", &rows);
    out.finish()
}
