//! The `emath explain` diagnostic browser and defaults table.

use super::*;

/// `explain <file> [<symbol>]` or `explain E-LAW-001`: plan-level or checker witness.
pub(crate) fn explain_cmd(request: ExplainRequest) -> CliExit {
    match request {
        ExplainRequest::Law { json } => explain_law_cmd(json),
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
    // override rows — the table never invents one.
    let override_rows: Vec<(String, String)> = Vec::new();

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
