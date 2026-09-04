//! `emath check`: admission, data verification, and source reading.

use super::*;

/// `check <file> [--verify-data] [--json]`: parse + admit, no codegen.
/// `--verify-data` (04 §5.2) re-hashes every
/// `sha256` declared in InstrumentRun provenance against the file on
/// disk, relative to the source file; drift refuses `E-OBS-HASH`.
pub fn check(path: &Path, json: bool, verify_data: bool) -> CliExit {
    if let Some(code) = meaning_cmd::refuse_malformed_project_lock(path) {
        return code;
    }
    let path = path.to_path_buf();
    let (mut diagnostics, package_id, units_profiles) = run_check(&path);
    if verify_data && !diagnostics.has_errors() {
        verify_declared_data(&path, &mut diagnostics);
    }
    let meaning_id = if diagnostics.has_errors() {
        None
    } else {
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|source| admitted_meaning_id(&path, &source))
    };
    print_diagnostics(&diagnostics);
    if !json && !units_profiles.is_empty() {
        // §6.5 pack-table: the effective honesty declaration, printed
        // deterministically in source order (admission order).
        for (declaration, profile) in &units_profiles {
            println!("honesty: units_profile {declaration}={profile}");
        }
    }
    if json {
        // The diagnostics array carries codes and messages, not counts:
        // a checker lane must be able to assert the exact E-* code the
        // CLI refused with.
        println!(
            "{}",
            check_json_document(
                !diagnostics.has_errors(),
                &package_id,
                &diagnostics,
                meaning_id.as_ref().map(|id| id.as_str()),
                &units_profiles,
            )
        );
    }
    exit_from_diagnostics(diagnostics.has_errors())
}

/// Declared raw-data digests (04 §5.2): InstrumentRun provenance rows
/// carrying a `sha256`, as (binding, file, declared digest).
pub(super) fn declared_data_digests(path: &Path) -> Vec<(String, String, String)> {
    let mut session = CompilerSession::new(emath_core::limits::Limits::default());
    let Ok(package) = session.load_package(path) else {
        return Vec::new();
    };
    let result = session.check(package.file);
    result
        .package
        .binding_provenance
        .iter()
        .filter_map(|(site, provenance)| match provenance {
            emath_ir::Provenance::InstrumentRun {
                file,
                sha256: Some(sha256),
                ..
            } => Some((site.binding.clone(), file.clone(), sha256.clone())),
            _ => None,
        })
        .collect()
}

/// Re-hash every declared data digest; append `E-OBS-HASH` on drift or
/// unreadable data. Changed data under an unchanged model is a different
/// artifact identity.
pub(super) fn verify_declared_data(path: &Path, diagnostics: &mut Diagnostics) {
    let base = path.parent().map(Path::to_path_buf).unwrap_or_default();
    for (binding, file, declared) in declared_data_digests(path) {
        let data_path = base.join(&file);
        let Some(bytes) = std::fs::read(&data_path).ok() else {
            diagnostics.error(
                "E-OBS-HASH",
                format!(
                    "cannot read data file for observation `{binding}` ({})",
                    data_path.display()
                ),
                emath_core::Span::default(),
            );
            continue;
        };
        let digest: String = emath_core::sha256_digest(&bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        if !declared.eq_ignore_ascii_case(&digest) {
            diagnostics.error(
                "E-OBS-HASH",
                format!(
                    "data drift for observation `{binding}`: declared sha256 {declared} but {} hashes to {digest} — changed data under an unchanged model is a different artifact identity",
                    data_path.display()
                ),
                emath_core::Span::default(),
            );
        }
    }
}

pub(super) fn admitted_meaning_id(path: &Path, source: &str) -> Option<emath_core::MeaningId> {
    let mut session = CompilerSession::new(emath_core::limits::Limits::default());
    let result = session.check_owned(&path.display().to_string(), source);
    if result.diagnostics.has_errors() {
        return None;
    }
    result.package.meaning_id(&[]).ok()
}

pub(super) fn source_has_content(source: &str) -> bool {
    source
        .lines()
        .map(str::trim)
        .any(|line| !line.is_empty() && !line.starts_with('#') && !line.starts_with("//"))
}

pub(super) fn read_emath_source(command: &str, path: &Path, json: bool) -> Result<String, CliExit> {
    match std::fs::read_to_string(path) {
        Ok(source) => {
            if source_has_content(&source) {
                Ok(source)
            } else {
                Err(refuse_coded(
                    command,
                    json,
                    EXIT_REFUSED,
                    "E-PKG-081",
                    &format!("source has no declarations ({})", path.display()),
                ))
            }
        }
        Err(_) => Err(refuse_coded(
            command,
            json,
            EXIT_USAGE,
            "E-PKG-080",
            &format!("cannot read source file ({})", path.display()),
        )),
    }
}

pub(super) fn print_missing_newline(s: &str) {
    if !s.ends_with('\n') {
        println!();
    }
}
