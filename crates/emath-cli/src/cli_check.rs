//! `emath check`: admission, data verification, and source reading.

use super::{Path, CliExit, check_json_document, exit_from_diagnostics, print_json_diagnostics, json_diagnostic_entry, EXIT_USAGE, EXIT_IO, Diagnostics};

/// `check <file> [--verify-data] [--json]`: parse + admit, no codegen.
/// `--verify-data` re-hashes every
/// `sha256` declared in `InstrumentRun` provenance against the file on
/// disk, relative to the source file; drift refuses `E-OBS-HASH`.
pub fn check(path: &Path, json: bool, verify_data: bool) -> CliExit {
    if path.as_os_str() == "-" {
        return check_stdin(json, verify_data);
    }
    let path = path.to_path_buf();
    let checked = crate::cli_build::checked_file(&path);
    let package_id = if checked.store.files().is_empty() {
        String::new()
    } else {
        checked.result.package.content_id().0
    };
    let mut diagnostics = checked.result.diagnostics;
    let units_profiles = checked.result.units_profiles;
    if verify_data && !diagnostics.has_errors() {
        verify_declared_data(&path, &checked.result.package, &mut diagnostics);
    }
    let meaning_id = if diagnostics.has_errors() {
        None
    } else {
        checked.result.package.meaning_id(&[]).ok()
    };
    crate::cli_json::print_diagnostics_from(&diagnostics, &checked.store);
    if !json && !units_profiles.is_empty() {
        // pack-table: the effective honesty declaration, printed
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
                meaning_id.as_ref().map(emath_core::MeaningId::as_str),
                &units_profiles,
                Some(&checked.store),
            )
        );
    }
    exit_from_diagnostics(diagnostics.has_errors())
}

/// `check -`: read `.emath` source from stdin (unix pipeline
/// composability, e.g. `emath fmt - < f.emath | emath check -`).
/// Deterministic; the package id derives from the source bytes, so a
/// piped check matches the same text checked from disk.
///
/// No data-file base directory exists for stdin, so
/// `--verify-data` is refused (`E-CLI-USAGE`): `InstrumentRun` digests
/// resolve relative to a source file on disk.
fn check_stdin(json: bool, verify_data: bool) -> CliExit {
    use std::io::Read;
    if verify_data {
        let message =
            "--verify-data requires a file path; stdin source has no data-file base directory";
        eprintln!("error: E-CLI-USAGE: {message}");
        if json {
            print_json_diagnostics(
                "check",
                false,
                &[json_diagnostic_entry("E-CLI-USAGE", "error", message)],
            );
        }
        return EXIT_USAGE;
    }
    let mut source = String::new();
    if std::io::stdin().read_to_string(&mut source).is_err() {
        let message = "cannot read `.emath` source from stdin";
        eprintln!("error: E-PKG-080: {message}");
        if json {
            print_json_diagnostics(
                "check",
                false,
                &[json_diagnostic_entry("E-PKG-080", "error", message)],
            );
        }
        return EXIT_IO;
    }
    let checked = crate::cli_build::checked_text("<stdin>", &source);
    let package_id = checked.result.package.content_id().0;
    let diagnostics = checked.result.diagnostics;
    let units_profiles = checked.result.units_profiles;
    let meaning_id = if diagnostics.has_errors() {
        None
    } else {
        checked.result.package.meaning_id(&[]).ok()
    };
    crate::cli_json::print_diagnostics_from(&diagnostics, &checked.store);
    if !json && !units_profiles.is_empty() {
        // pack-table, same contract as file mode.
        for (declaration, profile) in &units_profiles {
            println!("honesty: units_profile {declaration}={profile}");
        }
    }
    if json {
        println!(
            "{}",
            check_json_document(
                !diagnostics.has_errors(),
                &package_id,
                &diagnostics,
                meaning_id.as_ref().map(emath_core::MeaningId::as_str),
                &units_profiles,
                Some(&checked.store),
            )
        );
    }
    exit_from_diagnostics(diagnostics.has_errors())
}

/// Declared raw-data digests: `InstrumentRun` provenance rows
/// carrying a `sha256`, as (binding, file, declared digest).
fn declared_data_digests(package: &emath_ir::SemanticPackage) -> Vec<(String, String, String)> {
    package
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
pub(super) fn verify_declared_data(path: &Path, package: &emath_ir::SemanticPackage, diagnostics: &mut Diagnostics) {
    let base = path.parent().map(Path::to_path_buf).unwrap_or_default();
    for (binding, file, declared) in declared_data_digests(package) {
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
                    "data drift for observation `{binding}`: declared sha256 {declared} but {} hashes to {digest}; changed data under an unchanged model is a different artifact identity",
                    data_path.display()
                ),
                emath_core::Span::default(),
            );
        }
    }
}

pub fn admitted_meaning_id(path: &Path, source: &str) -> Option<emath_core::MeaningId> {
    let result = crate::cli_build::checked_text(&path.display().to_string(), source).result;
    if result.diagnostics.has_errors() {
        return None;
    }
    result.package.meaning_id(&[]).ok()
}
