use super::*;

pub(super) fn checked_package(path: &Path, json: bool) -> Result<SemanticPackage, CliExit> {
    let mut session = CompilerSession::new(Limits::default());
    let loaded = session
        .load_package(path)
        .map_err(|error| diagnostic(json, EXIT_USAGE, "E-PKG-080", &format!("{error:?}")))?;
    let result = session.check(loaded.file);
    if result.diagnostics.has_errors() {
        crate::print_diagnostics(&result.diagnostics);
        if json {
            let mut out = JsonWriter::object();
            out.string("schema_version", "emath.constructor.v1");
            out.string("command", "check");
            out.string("admission", "refused");
            out.objects("evidence", &[]);
            out.objects(
                "diagnostics",
                &crate::json_diagnostics_entries(&result.diagnostics),
            );
            println!("{}", out.finish());
        }
        Err(EXIT_REFUSED)
    } else {
        Ok(result.package)
    }
}

pub(super) fn installed_language(path: &Path, json: bool) -> Result<String, CliExit> {
    let distribution = crate::cli_dispatch::load_verified_language(Some(path))
        .map_err(|detail| diagnostic(json, EXIT_REFUSED, "E-LANG-IMAGE", &detail))?;
    Ok(distribution.image.distribution_hash.to_string())
}

/// Historical planner package. Live `emath run` never calls this.
pub(super) fn planned_package(path: &Path, json: bool) -> Result<SemanticPackage, CliExit> {
    let mut session = CompilerSession::new(Limits::default());
    let loaded = session
        .load_package(path)
        .map_err(|error| diagnostic(json, EXIT_USAGE, "E-PKG-080", &format!("{error:?}")))?;
    let result = session.plan(loaded.file);
    if result.diagnostics.has_errors() {
        crate::print_diagnostics(&result.diagnostics);
        if json {
            let mut out = JsonWriter::object();
            out.string("schema_version", "emath.constructor.v1");
            out.string("command", "run");
            out.string("admission", "refused");
            out.objects(
                "diagnostics",
                &crate::json_diagnostics_entries(&result.diagnostics),
            );
            println!("{}", out.finish());
        }
        Err(EXIT_REFUSED)
    } else {
        Ok(result.package)
    }
}

