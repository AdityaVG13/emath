//! Diagnostic printing and JSON document helpers shared by CLI commands.

use super::*;

pub fn print_diagnostics(diagnostics: &Diagnostics) {
    for item in diagnostics.items() {
        eprintln!(
            "{} {} ({}:{})",
            item.code, item.message, item.primary.file.0, item.primary.start
        );
        if let Some(help) = &item.help {
            for line in help.lines() {
                eprintln!("  {line}");
            }
        }
    }
}

/// Split `error: E-FOO-001: rest` (or the same without the `error:` prefix)
/// into a stable code and message.
pub(crate) fn split_error_code(error: &str) -> Option<(&str, &str)> {
    let error = error.strip_prefix("error: ").unwrap_or(error).trim();
    let (code, rest) = error.split_once(':')?;
    let code = code.trim();
    if code.starts_with("E-") || code.starts_with("N-") {
        Some((code, rest.trim()))
    } else {
        None
    }
}

/// One `{code,severity,message}` diagnostic object for `--json` envelopes.
pub fn json_diagnostic_entry(code: &str, severity: &str, message: &str) -> String {
    let mut entry = emath_artifact::JsonWriter::object();
    entry.string("code", code);
    entry.string("severity", severity);
    entry.string("message", message);
    entry.finish().trim_end().to_string()
}

pub(super) fn json_put_opt(entry: &mut emath_artifact::JsonObject, key: &str, value: Option<&str>) {
    if let Some(value) = value {
        entry.string(key, value);
    }
}

pub(crate) fn json_diagnostics_entries(diagnostics: &Diagnostics) -> Vec<String> {
    diagnostics
        .items()
        .iter()
        .map(|item| {
            let mut entry = emath_artifact::JsonWriter::object();
            entry.string("code", item.code);
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
pub fn diagnostics_json_document(command: &str, admitted: bool, entries: &[String]) -> String {
    let mut out = emath_artifact::JsonWriter::object();
    out.string("command", command);
    out.bool("admitted", admitted);
    out.objects("diagnostics", entries);
    out.finish()
}

pub(crate) fn print_json_diagnostics(command: &str, admitted: bool, entries: &[String]) {
    println!("{}", diagnostics_json_document(command, admitted, entries));
}

pub(super) fn refuse_coded(
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
) -> String {
    let mut out = emath_artifact::JsonWriter::object();
    out.string("command", "check");
    out.bool("admitted", admitted);
    out.objects("diagnostics", &json_diagnostics_entries(diagnostics));
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

pub(super) fn goal_json_rows(goals: &[emath_ir::Goal]) -> Vec<String> {
    goals
        .iter()
        .map(|goal| {
            let mut row = emath_artifact::JsonWriter::object();
            row.string("kind", goal.kind.as_str());
            row.string("target", &goal.target);
            row.finish().trim_end().to_string()
        })
        .collect()
}
