//! The `emath explain` diagnostic browser and defaults table.

use super::*;

/// `explain <file> [<symbol>]` or `explain E-LAW-001`: plan-level or checker witness.
pub(crate) fn explain_cmd(request: ExplainRequest) -> CliExit {
    match request {
        ExplainRequest::Law { json } => explain_law_cmd(json),
        ExplainRequest::Code { code, json } => explain_code_cmd(code.as_deref(), json),
        ExplainRequest::File {
            path,
            symbol,
            provenance,
            json,
            show_defaults,
        } => {
            if provenance {
                return match crate::provenance_explanation(&path, json) {
                    Ok(explanation) => {
                        print!("{explanation}");
                        EXIT_OK
                    }
                    Err(code) => code,
                };
            }
            if show_defaults {
                return show_defaults_cmd(&path, json);
            }
            let inspections = match crate::explain_inspections(&path) {
                Ok(inspections) => inspections,
                Err(code) => return code,
            };
            if json {
                for inspection in &inspections {
                    println!("{}", inspection.to_json());
                }
                return EXIT_OK;
            }
            for inspection in &inspections {
                println!("{}", inspection.explain());
            }
            if let Some(symbol) = symbol {
                println!(
                    "explain: symbol `{symbol}`: declaration indexing is Phase 4+; goals above are the available evidence"
                );
            }
            EXIT_OK
        }
    }
}

/// `(code, title, cause, remediation)` rows for every diagnostic code
/// the CLI itself emits (compiler-emitted `E-*` codes are owned by
/// `language/reference/` and refuse with a pointer there). Exact-match
/// lookup; the row order is the stable registry order.
pub(super) const DIAGNOSTIC_ROWS: [(&str, &str, &str, &str); 20] = [
    (
        "E-CLI-USAGE",
        "invalid or missing command arguments",
        "the arguments after the command name do not match the command's usage line",
        "run `emath <command> --help` and follow the usage line exactly",
    ),
    (
        "E-CLI-UNKNOWN-COMMAND",
        "unknown subcommand",
        "the first token is not a command or alias in the catalog",
        "run `emath help` for the full command list; near-misses suggest the closest command",
    ),
    (
        "E-CLI-UNKNOWN-FLAG",
        "unknown flag for this command",
        "the flag is not in the command's accepted flag set",
        "run `emath <command> --help`; near-misses suggest the closest flag",
    ),
    (
        "E-CLI-MISSING-VALUE",
        "flag requires a value",
        "a value-taking flag was passed without its value",
        "spell `--flag value`; the help usage line shows every value-taking flag",
    ),
    (
        "E-CLI-INVALID-ARGUMENT",
        "flag value outside its accepted set",
        "the value after the flag is not one of the documented choices",
        "use one of the documented values (see `emath <command> --help`)",
    ),
    (
        "E-LANG-IMAGE",
        "verified Language Image refused",
        "no verified Language Image could be loaded from language/spec for this source",
        "run inside the project so `language/spec` is discoverable; `emath doctor` reports the probe state",
    ),
    (
        "E-PKG-080",
        "cannot read source file",
        "the `.emath` path does not exist or is unreadable",
        "check the path spelling; `emath check -` reads from stdin instead",
    ),
    (
        "E-PKG-081",
        "package lock malformed",
        "an `emath.package.lock` file next to the source failed to parse or verify",
        "delete nothing; run `emath meaning explain` to inspect the lock state",
    ),
    (
        "E-TLT-005",
        "toolchain operation failed",
        "the toolchain lane (vendor/fork/provider) reported an upstream failure",
        "run `emath doctor` for toolchain health; `emath provider list` shows lane status",
    ),
    (
        "E-TLT-010",
        "invalid package name",
        "`emath new` names must start with a letter and use only letters, digits, `_`, `-`",
        "spell `emath new my_model` (ASCII letter first)",
    ),
    (
        "E-TLT-011",
        "refusing to overwrite existing project",
        "the target directory already contains a scaffold (exit code 5 = safety)",
        "pick a fresh `--out` directory, or pass `--force` to overwrite deliberately",
    ),
    (
        "E-OBS-HASH",
        "declared data drift",
        "an InstrumentRun sha256 no longer matches the data file on disk; changed data under an unchanged model is a different artifact identity",
        "restore the data file, or re-declare the sha256 in the provenance row after verifying the new data",
    ),
    (
        "E-RUN-STATE",
        "execution state violation",
        "the checkpoint or run state does not permit the requested operation",
        "pass the latest checkpoint (`emath step <checkpoint.json>`); `emath inspect <checkpoint.json> --json` shows the current state",
    ),
    (
        "E-RUN-DRIFT",
        "checkpoint drift",
        "the checkpoint file changed since the run started",
        "restart the run or branch with `--branch-from checkpoint --relation relation`",
    ),
    (
        "E-RUN-REVISION",
        "checkpoint revision conflict",
        "a concurrent writer changed the checkpoint (--expect-revision guard)",
        "re-read the checkpoint and pass its current `--expect-revision N`",
    ),
    (
        "E-RUN-CANCELLED",
        "run cancelled",
        "the sentinel file named by `--cancel-file` appeared during execution",
        "remove the sentinel to allow new runs; this is a clean stop, not a crash",
    ),
    (
        "E-DIFF-FAILED",
        "diff refused",
        "one of the two sources could not be read or admitted",
        "run `emath check` on each input first; diff reports are content-id based",
    ),
    (
        "E-SYM-002",
        "symbol not found",
        "the named declaration does not exist in the source",
        "check the spelling against `emath explain <file>` plan output",
    ),
    (
        "E-SYM-003",
        "ambiguous symbol",
        "the name matches more than one declaration in scope",
        "qualify the symbol with its declaration-qualified name",
    ),
    (
        "E-LOCK-004",
        "meaning lock conflict",
        "the project's `.emath/meaning.lock` disagrees with the current source",
        "run `emath meaning explain` to see the divergence; never hand-edit the lock",
    ),
];

/// `explain [<E-CODE>] [--list-codes] [--json]`: registry lookup with
/// cause and copy-pasteable remediation. No argument + `--list-codes`
/// lists the whole registry in stable order. A miss refuses with the
/// known namespaces and points compiler-emitted codes at
/// `language/reference/` (the language owns those diagnostics).
fn explain_code_cmd(code: Option<&str>, json: bool) -> CliExit {
    let Some(code) = code else {
        // List mode: every registry code, one per line; JSON emits the
        // full rows so an agent can pick without a second call.
        if json {
            let mut out = JsonWriter::object();
            out.string("command", "explain-list-codes");
            out.string("schema", "emath.diagnostic-registry");
            let rows: Vec<String> = DIAGNOSTIC_ROWS
                .iter()
                .map(|(code, title, cause, remediation)| {
                    let mut row = JsonWriter::object();
                    row.string("code", code);
                    row.string("title", title);
                    row.string("cause", cause);
                    row.string("remediation", remediation);
                    row.finish().trim_end().to_string()
                })
                .collect();
            out.objects("diagnostics", &rows);
            println!("{}", out.finish());
        } else {
            println!(
                "emath diagnostic registry: {} codes emitted by this CLI",
                DIAGNOSTIC_ROWS.len()
            );
            for (code, title, _, _) in DIAGNOSTIC_ROWS {
                println!("{code}\t{title}");
            }
            println!("compiler-emitted E-* codes: see language/reference/");
        }
        return EXIT_OK;
    };
    if json {
        let mut out = JsonWriter::object();
        out.string("command", "explain");
        out.string("schema", "emath.diagnostic-explanation");
        if let Some((found_code, title, cause, remediation)) =
            DIAGNOSTIC_ROWS.iter().find(|row| row.0 == code)
        {
            out.bool("known", true);
            out.string("code", found_code);
            out.string("title", title);
            out.string("cause", cause);
            out.string("remediation", remediation);
        } else {
            out.bool("known", false);
            out.string("code", code);
            let mut namespaces = std::collections::BTreeSet::new();
            for (registry_code, _, _, _) in DIAGNOSTIC_ROWS {
                namespaces.insert(
                    registry_code
                        .split('-')
                        .take(2)
                        .collect::<Vec<_>>()
                        .join("-"),
                );
            }
            let namespace_list: Vec<String> = namespaces.into_iter().collect();
            out.strings("cli_namespaces", &namespace_list);
            out.string(
                "remediation",
                "compiler-emitted E-* codes are documented in language/reference/; run `emath explain --list-codes --json` for the CLI registry",
            );
        }
        println!("{}", out.finish());
        return EXIT_OK;
    }
    if let Some((found_code, title, cause, remediation)) = DIAGNOSTIC_ROWS.iter().find(|row| row.0 == code) {
        println!("{found_code}: {title}");
        println!("  cause: {cause}");
        println!("  fix:   {remediation}");
        return EXIT_OK;
    }
    eprintln!("error: E-CLI-USAGE: unknown diagnostic code `{code}`");
    eprintln!("  what:  the CLI registry covers only codes emath-cli itself emits");
    eprintln!("  where: compiler-emitted E-* codes are documented in language/reference/");
    eprintln!("  fix:   run `emath explain --list-codes` for the registry");
    EXIT_USAGE
}

/// F8: the effective-defaults table.
/// Every implicit default the compiler applies, each labeled with its
/// source (`language default` / `declaration attribute` / `planner
/// default`) and, where one exists, the explicit override spelling.
/// Deterministic: fixed row order, no map iteration.
pub(super) fn show_defaults_cmd(path: &Path, json: bool) -> CliExit {
    let mut session = CompilerSession::new(emath_core::limits::Limits::default());
    let Ok(package) = session.load_package(path) else {
        eprintln!("error: cannot read {}", path.display());
        return EXIT_USAGE;
    };
    let result = session.check(package.file);
    print_diagnostics(&result.diagnostics);
    if result.diagnostics.has_errors() {
        return EXIT_REFUSED;
    }

    // One row per declaration that OVERRIDES a default (a declared
    // units profile), in source order. A file with no overrides has no
    // override rows — the table never invents one. The checker's
    // units_profiles table is exactly those declarations, in admission
    // order.
    let override_rows: Vec<(String, String)> = result.units_profiles.clone();

    if json {
        let mut out = JsonWriter::object();
        out.string("command", "explain-show-defaults");
        let rows: Vec<String> = DEFAULTS_ROWS
            .iter()
            .map(|(default, value, source, r#override)| {
                let mut row = JsonWriter::object();
                row.string("default", default);
                row.string("value", value);
                row.string("source", source);
                row.string("override", r#override);
                row.finish().trim_end().to_string()
            })
            .collect();
        out.objects("defaults", &rows);
        let overrides: Vec<String> = override_rows
            .iter()
            .map(|(declaration, profile)| {
                let mut row = JsonWriter::object();
                row.string("default", "units-profile");
                row.string("declaration", declaration);
                row.string("value", profile);
                row.string("source", "declaration attribute");
                row.string("override", "@units_profile(<level>)");
                row.finish().trim_end().to_string()
            })
            .collect();
        out.objects("declaration_overrides", &overrides);
        println!("{}", out.finish());
        return EXIT_OK;
    }

    println!(
        "emath explain --show-defaults: {} effective defaults",
        DEFAULTS_ROWS.len()
    );
    for (default, value, source, r#override) in DEFAULTS_ROWS {
        if r#override.is_empty() {
            println!("{default}: {value} (source: {source})");
        } else {
            println!("{default}: {value} (source: {source}; override: {override})");
        }
    }
    for (declaration, profile) in &override_rows {
        println!(
            "units-profile: {declaration}={profile} (source: declaration attribute; override: \
             @units_profile(<level>))"
        );
    }
    EXIT_OK
}

/// `(default, value, source, override)` rows. `override` is empty when no
/// explicit surface exists. Values must agree with the code that applies
/// them: `NumericProfile::default_phase1` (strict-f64), the permissive
/// units ladder floor (ch. 5), `Visibility::Public` admission default,
/// outputs-default-to-definitions, the `compile:` defaults, the
/// untyped-input `Float64` fallback (`N-TYPE-001`), and
/// `PlannerConfig::default`.
pub(super) const DEFAULTS_ROWS: [(&str, &str, &str, &str); 7] = [
    (
        "numeric-profile",
        "strict-f64",
        "language default",
        "compile: numeric <name> (E-NUM-001 on unknown)",
    ),
    (
        "units-profile",
        "permissive (no profile refusal floor)",
        "language default",
        "@units_profile(permissive|lab|engineering|publication)",
    ),
    (
        "visibility",
        "public",
        "language default",
        "spell `pub` on the item to make it explicit",
    ),
    (
        "outputs",
        "all definitions",
        "language default (outputs: omitted)",
        "outputs: section",
    ),
    (
        "compile",
        "target rust, profile library, numeric strict-f64",
        "language default (compile: omitted)",
        "compile: section",
    ),
    (
        "untyped-inputs",
        "Float64",
        "language default (N-TYPE-001 notice at admission)",
        "annotate the input with a type",
    ),
    (
        "planner",
        "policy deterministic-planner, max_candidates 8, max_nodes 16, tie-break \
         cost-ascending-id",
        "planner default",
        "",
    ),
];

pub(super) fn explain_law_cmd(json: bool) -> CliExit {
    let (report, explanations) = crate::diagnostics::e_law_001_demo();
    if report.passed {
        eprintln!("error: E-LAW-001 demo table unexpectedly held");
        return EXIT_REFUSED;
    }
    let Some(explanation) = explanations.first() else {
        eprintln!("error: checker produced no witness");
        return EXIT_REFUSED;
    };
    if let Err(error) = crate::diagnostics::tutor_check_v1(explanation) {
        eprintln!("error: tutor-check/v1 refused ({})", error.as_str());
        return EXIT_REFUSED;
    }
    if json {
        print!("{}", crate::diagnostics::explanation_json(explanation));
        return EXIT_OK;
    }
    println!("{} {}", explanation.code, explanation.kind.as_str());
    println!("{}", explanation.structured_narrative);
    if let Some(witness) = &explanation.witness {
        print!("{}", crate::diagnostics::render_cayley_ascii(witness));
    }
    EXIT_OK
}
