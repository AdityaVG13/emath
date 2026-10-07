//! `emath diff`: source fingerprint comparison.

use super::{Path, CliExit, EXIT_REFUSED, JsonWriter, EXIT_OK, EXIT_IO, CompilerSession};

/// `diff <a.emath> <b.emath>`: fingerprint comparison of parse-admitted sources.
pub(crate) fn diff_cmd(a: &Path, b: &Path, json: bool) -> CliExit {
    let id_a = fingerprint(a);
    let id_b = fingerprint(b);
    match (id_a, id_b) {
        (Ok(id_a), Ok(id_b)) => {
            let identical = id_a == id_b;
            if json {
                let mut object = JsonWriter::object();
                object.string("schema", "emath.diff");
                object.string("a", &a.display().to_string());
                object.string("a_id", &id_a.0);
                object.string("b", &b.display().to_string());
                object.string("b_id", &id_b.0);
                object.bool("identical", identical);
                println!("{}", object.finish());
            } else {
                println!("diff: {} {}", a.display(), id_a.0);
                println!("diff: {} {}", b.display(), id_b.0);
                println!("diff: {}", if identical { "identical" } else { "differ" });
            }
            if identical { EXIT_OK } else { EXIT_REFUSED }
        }
        (Err(()), _) | (_, Err(())) => {
            if json {
                crate::print_json_diagnostics(
                    "diff",
                    false,
                    &[crate::json_diagnostic_entry(
                        "E-DIFF-FAILED",
                        "error",
                        "fingerprint comparison failed: cannot read input file",
                    )],
                );
            }
            EXIT_IO
        }
    }
}

/// Content id of a parse-admitted source; diagnostics printed on refusal.
pub(super) fn fingerprint(file: &Path) -> Result<emath_core::ContentId, ()> {
    let mut session = CompilerSession::new(emath_core::limits::Limits::default());
    let Ok(package) = session.load_package(file) else {
        eprintln!("error: cannot read {}", file.display());
        return Err(());
    };
    let result = session.check(package.file);
    crate::cli_json::print_diagnostics_from(&result.diagnostics, &session.store);
    if result.diagnostics.has_errors() {
        return Err(());
    }
    Ok(emath_core::bootstrap_content_id(package.text.as_bytes()))
}
