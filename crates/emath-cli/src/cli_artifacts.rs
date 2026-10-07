//! Published-artifact listing and checking, and generated help text.

use super::{Path, CliExit, EXIT_USAGE, EXIT_REFUSED, EXIT_OK, catalog};

pub fn list_published_artifact_ids(dir: &Path) -> Result<Vec<String>, CliExit> {
    let artifact_root = dir.join("emath");
    if !artifact_root.is_dir() {
        eprintln!(
            "error: E-EVID-105: no `emath/` state directory under {}",
            dir.display()
        );
        return Err(EXIT_USAGE);
    }
    let Ok(entries) = std::fs::read_dir(&artifact_root) else {
        eprintln!(
            "error: E-TLT-005: cannot list artifact state directory {}",
            artifact_root.display()
        );
        return Err(EXIT_USAGE);
    };
    let mut artifact_ids = Vec::new();
    for entry in entries.flatten() {
        if entry.path().is_dir() {
            artifact_ids.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    artifact_ids.sort_unstable();
    if artifact_ids.is_empty() {
        eprintln!(
            "error: E-EVID-105: no published artifacts under {}",
            artifact_root.display()
        );
        return Err(EXIT_REFUSED);
    }
    Ok(artifact_ids)
}

/// `artifact check <dir>`: independent verification of every published
/// artifact under `<dir>/emath/<artifact-id>` via `emath-evidence`'s `checker` module
/// (one identity, one checker; empty state dirs are refused).
pub fn artifact_check(dir: &Path) -> CliExit {
    let artifact_ids = match list_published_artifact_ids(dir) {
        Ok(ids) => ids,
        Err(code) => return code,
    };
    let artifact_root = dir.join("emath");
    let mut ok = true;
    for id in artifact_ids {
        let root = artifact_root.join(&id);
        match emath_evidence::checker::check_artifact_dir(&root) {
            Ok(report) if report.valid() => {
                println!("artifact {id}: verified ({} files)", report.files_verified);
            }
            Ok(report) => {
                for issue in &report.issues {
                    eprintln!("artifact {id}: {}: {}", issue.code, issue.message);
                }
                ok = false;
            }
            Err(error) => {
                eprintln!("artifact {id}: FAILED: {error}");
                ok = false;
            }
        }
    }
    if ok { EXIT_OK } else { EXIT_REFUSED }
}

/// `help` output. Generated from the command catalog so usage and summary
/// cannot drift from `emath help <command>` / `emath capabilities --json`.
pub fn help_text() -> String {
    let mut out = String::from("emath compiler (constructor layer). Usage:\n");
    for command in catalog::COMMANDS {
        let Some(usage) = catalog::command_usage(command) else {
            continue;
        };
        let Some(summary) = catalog::command_summary(command) else {
            continue;
        };
        out.push_str("  emath ");
        out.push_str(usage);
        out.push('\n');
        out.push_str("      ");
        out.push_str(summary);
        out.push('\n');
        let aliases = catalog::command_aliases(command);
        if !aliases.is_empty() {
            out.push_str("      aliases: ");
            out.push_str(&aliases.join(", "));
            out.push('\n');
        }
    }
    out.push_str("\nexit codes: 0 completed, 2 admission, 3 unmet/partial, 4 fault, 5 checkpoint\n");
    out
}
