//! Fit goal surface (04 §5.3) — generic
//! fit grammar / AST / admission / lowering. `fit <params> to
//! <observable>:` admits as a goal whose suite carries plain program
//! data: `model` path, `prediction` label, `residual:` method,
//! `method` optimizer, `initial:` seeds, `weights:` explicit weights,
//! and the `require identifiability.structural` honesty gate. No
//! domain model is bound in the compiler: the PK two-compartment fit is
//! the runnable fixture `language/examples/science/
//! pk-two-compartment-fit.emath` (byte-canonical under the formatter),
//! execution uses generic capability/method/provider seams, and every
//! plan excludes a missing structural-identifiability provider with a
//! typed reason.

use emath_core::limits::Limits;
use emath_ir::goal::GoalPayload;
use emath_sema::session::CompilerSession;

const PK_FIT_FIXTURE: &str =
    include_str!("../../../language/examples/science/pk-two-compartment-fit.emath");

fn check(text: &str, name: &str) -> Vec<String> {
    Source::from_str(name, text)
        .check()
        .diagnostics
        .errors()
        .map(|d| format!("{} {}", d.code, d.message))
        .collect()
}

fn plan_requests(text: &str, name: &str) -> (Vec<String>, Vec<(String, String)>) {
    let mut session = CompilerSession::new(Limits::default());
    let file = session.load_text(name, text);
    let result = session.plan(file);
    let diagnostics: Vec<String> = result
        .diagnostics
        .errors()
        .map(|d| format!("{} {}", d.code, d.message))
        .collect();
    let requests = result
        .requests
        .iter()
        .map(|request| (request.kind.clone(), request.target.clone()))
        .collect();
    (diagnostics, requests)
}

/// Plans a text and returns the fit request's whole payload (or `None`
/// when the plan produced no fit request).
fn plan_fit_payload(text: &str, name: &str) -> (Vec<String>, Option<GoalPayload>) {
    let mut session = CompilerSession::new(Limits::default());
    let file = session.load_text(name, text);
    let result = session.plan(file);
    let diagnostics: Vec<String> = result
        .diagnostics
        .errors()
        .map(|d| format!("{} {}", d.code, d.message))
        .collect();
    let payload = result
        .requests
        .iter()
        .find(|request| request.kind == "fit")
        .map(|request| request.payload.clone());
    (diagnostics, payload)
}

const FIT_SINGLE: &str = "\
emath function pk_fit:
    inputs:
        t: Float64

    outputs:
        c: Float64
        conc_time: Float64

    definitions:
        c = 2.0
        conc_time = 0.0

    goals:
        fit k_el to conc_time:
            residual: weighted_least_squares
            method levenberg_marquardt
            initial: k_el = 0.2
";

const FIT_MISSING_RESIDUAL: &str = "\
emath function pk_fit:
    inputs:
        t: Float64

    outputs:
        c: Float64
        conc_time: Float64

    definitions:
        c = 2.0
        conc_time = 0.0

    goals:
        fit k_el to conc_time:
            method levenberg_marquardt
";

const FIT_MALFORMED_MISSING_TO: &str = "\
emath function pk_fit:
    inputs:
        t: Float64

    outputs:
        c: Float64
        conc_time: Float64

    definitions:
        c = 2.0
        conc_time = 0.0

    goals:
        fit k_el conc_time:
            residual: weighted_least_squares
            method levenberg_marquardt
";

const PLAIN_GOALS: &str = "\
emath function plain:
    inputs:
        x: Float64

    outputs:
        y: Float64

    definitions:
        y = x * 2.0
";

// ---- CLI end-to-end: `emath fit` parses, admits, plans, executes the
// declared fit to fitted values, and links Fitted provenance. The model
// math stays in the `.emath` fixture; the built binary is execed
// directly (see `common`) so the assertions cover what the CLI actually
// prints.

mod common;

fn fit_json_output(args: &[&str]) -> (emath_artifact::JsonValue, emath_cli::CliExit) {
    let output = std::process::Command::new(common::emath_bin())
        .args(["fit", "--json"])
        .args(args)
        .output()
        .expect("run emath fit binary");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let exit = match output.status.code() {
        Some(0) => emath_cli::CliExit::Ok,
        Some(1) => emath_cli::CliExit::Refused,
        _ => emath_cli::CliExit::Usage,
    };
    let parsed = match emath_artifact::parse_json_document(stdout.trim()) {
        Ok(parsed) => parsed,
        Err(error) => panic!("stdout must be a JSON envelope for `fit --json`: {error}"),
    };
    (parsed, exit)
}

use emath_artifact::JsonValue;

fn json_str<'a>(value: &'a JsonValue, name: &str) -> Option<&'a String> {
    match value {
        JsonValue::Obj(entries) => {
            entries
                .iter()
                .find(|(key, _)| key == name)
                .and_then(|(_, value)| match value {
                    JsonValue::Str(text) => Some(text),
                    _ => None,
                })
        }
        _ => None,
    }
}

fn json_bool(value: &JsonValue, name: &str) -> Option<bool> {
    match value {
        JsonValue::Obj(entries) => {
            entries
                .iter()
                .find(|(key, _)| key == name)
                .and_then(|(_, value)| match value {
                    JsonValue::Bool(flag) => Some(*flag),
                    _ => None,
                })
        }
        _ => None,
    }
}

fn json_array<'a>(value: &'a JsonValue, name: &str) -> Option<&'a Vec<JsonValue>> {
    match value {
        JsonValue::Obj(entries) => {
            entries
                .iter()
                .find(|(key, _)| key == name)
                .and_then(|(_, value)| match value {
                    JsonValue::Arr(items) => Some(items),
                    _ => None,
                })
        }
        _ => None,
    }
}

/// Parses the envelope's `parameters` object into (k_el, V_central).
fn fitted_numbers(envelope: &JsonValue) -> (f64, f64) {
    let JsonValue::Obj(entries) = envelope else {
        panic!("envelope must be an object");
    };
    match entries.iter().find(|(key, _)| key == "parameters") {
        Some((_, JsonValue::Obj(parameters))) => {
            let value_of = |name: &str| -> f64 {
                parameters
                    .iter()
                    .find(|(key, _)| key == name)
                    .and_then(|(_, value)| match value {
                        JsonValue::Num(text) => text.parse::<f64>().ok(),
                        _ => None,
                    })
                    .unwrap_or_else(|| panic!("parameter `{name}` missing"))
            };
            (value_of("k_el"), value_of("V_central"))
        }
        _ => panic!("envelope must carry a parameters object"),
    }
}

fn provenance_fit_id(row: &JsonValue) -> Option<String> {
    let JsonValue::Obj(entries) = row else {
        return None;
    };
    let (_, provenance) = entries.iter().find(|(key, _)| key == "provenance")?;
    let JsonValue::Obj(provenance_entries) = provenance else {
        return None;
    };
    match provenance_entries
        .iter()
        .find(|(key, _)| key == "kind")
        .map(|(_, value)| value)
    {
        Some(JsonValue::Str(kind)) if kind == "Fitted" => {}
        _ => return None,
    }
    match provenance_entries
        .iter()
        .find(|(key, _)| key == "fit_id")
        .map(|(_, value)| value)
    {
        Some(JsonValue::Str(fit_id)) => Some(fit_id.clone()),
        _ => None,
    }
}

use emath_test_harness::{Probe, Source, boot};

#[test]
fn fit_goal() {
    boot();
    let mut probe = Probe::new("Fit goal surface (04 §5.3) — generic fit grammar / AST / admission / lowering. `fit <params> to <observable>:` admits as a goal whose suite carries");
    probe.case("pk_fit_fixture_admits_and_lowers_to_fit_request", |p| {
    let f0 = p.failures().len();

    let errors = check(PK_FIT_FIXTURE, "pk-two-compartment-fit");
    p.demand("1",errors.is_empty(), format!(
        "the runnable PK fit fixture must admit with zero errors; got: {errors:#?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("fit_request_carries_full_generic_program_payload", |p| {
    let f0 = p.failures().len();

    let (diagnostics, payload) = plan_fit_payload(PK_FIT_FIXTURE, "pk-two-compartment-fit");
    p.demand("1",diagnostics.is_empty(), format!(
        "the PK fit fixture must plan with zero errors; got: {diagnostics:#?}"
    ));
    if p.failures().len() != f0 { return; }
    let payload = payload.expect("the fit request must carry its payload");
    p.demand("2", (payload.parameters) == (vec!["k_el", "V_central"]), format!("expected {:?}, got {:?}", (vec!["k_el", "V_central"]), (payload.parameters)));
    if p.failures().len() != f0 { return; }
    p.demand("3", (payload.model) == (vec!["PkTwoCompartment"]), format!("expected {:?}, got {:?}", (vec!["PkTwoCompartment"]), (payload.model)));
    if p.failures().len() != f0 { return; }
    p.demand("4", (payload.prediction) == ("central"), format!("expected {:?}, got {:?}", ("central"), (payload.prediction)));
    if p.failures().len() != f0 { return; }
    p.demand("5", (payload.residual) == ("weighted_least_squares"), format!("expected {:?}, got {:?}", ("weighted_least_squares"), (payload.residual)));
    if p.failures().len() != f0 { return; }
    p.demand("6", (payload.method) == ("levenberg_marquardt"), format!("expected {:?}, got {:?}", ("levenberg_marquardt"), (payload.method)));
    if p.failures().len() != f0 { return; }
    p.demand("7", (payload.initial) == (vec![
            ("k_el".to_string(), "0.2".to_string()),
            ("V_central".to_string(), "1.0".to_string())
        ]), format!("expected {:?}, got {:?}", (vec![
            ("k_el".to_string(), "0.2".to_string()),
            ("V_central".to_string(), "1.0".to_string())
        ]), (payload.initial)));
    if p.failures().len() != f0 { return; }
    p.eq("8", payload.weights.len(), 2);
    p.demand("9", (payload.data) == (vec![
            (
                "t".to_string(),
                vec![
                    "0.5".to_string(),
                    "1.0".to_string(),
                    "2.0".to_string(),
                    "4.0".to_string()
                ]
            ),
            (
                "conc_time".to_string(),
                vec![
                    "3.12".to_string(),
                    "2.43".to_string(),
                    "1.47".to_string(),
                    "0.54".to_string()
                ]
            ),
        ]), format!("expected {:?}, got {:?}", (vec![
            (
                "t".to_string(),
                vec![
                    "0.5".to_string(),
                    "1.0".to_string(),
                    "2.0".to_string(),
                    "4.0".to_string()
                ]
            ),
            (
                "conc_time".to_string(),
                vec![
                    "3.12".to_string(),
                    "2.43".to_string(),
                    "1.47".to_string(),
                    "0.54".to_string()
                ]
            ),
        ]), (payload.data)));
    if p.failures().len() != f0 { return; }
    p.demand("10",payload.require_identifiability, format!(
        "the honesty gate must survive lowering"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("single_param_fit_row_admits", |p| {
    let f0 = p.failures().len();

    let errors = check(FIT_SINGLE, "fit-single");
    p.demand("1",errors.is_empty(), format!(
        "a minimal fit goal must admit; got: {errors:#?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("fit_requires_residual_row_at_lowering", |p| {
    let f0 = p.failures().len();

    let (diagnostics, _) = plan_requests(FIT_MISSING_RESIDUAL, "fit-missing-residual");
    p.demand("1",diagnostics
            .iter()
            .any(|d| d.starts_with("E-GOAL-042") && d.contains("residual")), format!(
        "a fit without an explicit residual row must refuse at lowering; got: {diagnostics:#?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("malformed_fit_row_names_the_missing_to", |p| {
    let f0 = p.failures().len();

    let errors = check(FIT_MALFORMED_MISSING_TO, "fit-malformed");
    p.demand("1",errors
            .iter()
            .any(|e| e.starts_with("E-SYN-101") && e.contains("to")), format!(
        "a malformed fit row must name the expected `to`: got: {errors:#?}"
    ));
    if p.failures().len() != f0 { return; }
    p.demand("2",!errors
            .iter()
            .any(|e| e.contains("design follow-up") || e.contains("outside the Phase 1 subset")), format!(
        "the old design-fence refusal must be gone; got: {errors:#?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("plain_functions_still_admit", |p| {
    let f0 = p.failures().len();

    let errors = check(PLAIN_GOALS, "fit-plain-guard");
    p.demand("1",errors.is_empty(), format!(
        "the fit grammar must not affect ordinary functions; got: {errors:#?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("pk_fit_fixture_is_byte_canonical_under_the_formatter", |p| {
    let f0 = p.failures().len();

    let parsed =
        emath_syntax::parse_lossless(PK_FIT_FIXTURE, emath_core::FileId(0), &Limits::default());
    p.demand("1",!parsed.diagnostics.has_errors(), format!(
        "fixture must parse before formatting"
    ));
    if p.failures().len() != f0 { return; }
    let canonical = emath_syntax::formatter::format(&parsed.tree, &parsed.comments);
    p.eq("2", canonical.as_str(), PK_FIT_FIXTURE);

    });
    probe.case("fit_plan_excludes_missing_identifiability_provider", |p| {
    let f0 = p.failures().len();

    let mut session = CompilerSession::new(Limits::default());
    let file = session.load_text("pk-two-compartment-fit", PK_FIT_FIXTURE);
    let result = session.plan(file);
    let excluded = result
        .plans
        .iter()
        .flat_map(|plan| plan.excluded_candidates.iter())
        .filter(|candidate| candidate.provider == "fit.structural-identifiability")
        .collect::<Vec<_>>();
    p.demand("1",!excluded.is_empty(), format!(
        "every fit plan must exclude the missing structural-identifiability provider"
    ));
    if p.failures().len() != f0 { return; }
    p.demand("2",excluded
            .iter()
            .all(|candidate| candidate.reason.contains("unresolved")), format!(
        "the exclusion must carry the honest unresolved disposition"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("cli_fit_executes_pk_fixture_to_fitted_values_with_linked_provenance", |p| {
    let f0 = p.failures().len();

    let fixture = "../../language/examples/science/pk-two-compartment-fit.emath";
    let (envelope, exit) = fit_json_output(&[fixture]);
    p.eq("1", exit, emath_cli::CliExit::Ok);
    p.eq("2", json_str(&envelope, "command"), Some(&"fit".to_string()));
    p.eq("3", json_bool(&envelope, "admitted"), Some(true));
    p.eq("4", json_str(&envelope, "model"), Some(&"PkTwoCompartment".to_string()));
    p.eq("5", json_str(&envelope, "prediction"), Some(&"central".to_string()));
    p.eq("6", json_str(&envelope, "residual"), Some(&"weighted_least_squares".to_string()));
    p.eq("7", json_str(&envelope, "method"), Some(&"levenberg_marquardt".to_string()));
    let hash = json_str(&envelope, "hash")
        .expect("fitted envelope carries the content hash")
        .clone();
    p.eq("8", hash.len(), 16);
    let (measured_k_el, measured_v) = fitted_numbers(&envelope);
    p.demand("9",(measured_k_el - 0.5).abs() < 0.05 && (measured_v - 25.0).abs() < 3.0, format!(
        "the fixture data lies on C(t) = 100/V * exp(-k t) with k=0.5, V=25; \
         got k_el={measured_k_el}, V_central={measured_v}"
    ));
    if p.failures().len() != f0 { return; }
    let confidence =
        json_array(&envelope, "confidence").expect("granted fit carries confidence rows");
    p.eq("10", confidence.len(), 2);
    p.demand("11",confidence
            .iter()
            .all(|row| json_bool(row, "tight") == Some(true)), format!(
        "full-rank fixture data must certify every direction tight"
    ));
    if p.failures().len() != f0 { return; }
    let measured = json_array(&envelope, "measured")
        .expect("fitted envelope carries materialized measured values");
    p.eq("12", measured.len(), 2);
    for row in measured {
        let Some(fit_id) = provenance_fit_id(row) else {
            panic!("measured row must carry Fitted provenance with fit_id");
        };
        p.eq("13", &fit_id, &hash);
    }
    // Determinism: the same program + data + seed + method hashes
    // identically across runs.
    let (second, second_exit) = fit_json_output(&[fixture]);
    p.eq("14", second_exit, emath_cli::CliExit::Ok);
    p.eq("15", json_str(&second, "hash"), Some(&hash));

    });
    probe.case("cli_fit_refuses_a_fit_without_a_model_declaration", |p| {
    let f0 = p.failures().len();

    // A temporary negative file: fit goal naming a model that is not
    // declared. Written to the system temp dir — no repository
    // artifacts are produced.
    let mut path = std::env::temp_dir();
    path.push("emath-fit-negative-no-model.emath");
    std::fs::write(
        &path,
        "emath function NoModelFit:\n\
         \x20   inputs:\n\
         \x20       t: Float64\n\
         \n\
         \x20   outputs:\n\
         \x20       y: Float64\n\
         \n\
         \x20   definitions:\n\
         \x20       y = 0.0\n\
         \n\
         \x20   goals:\n\
         \x20       fit k to y:\n\
         \x20           model MissingModel\n\
         \x20           prediction y\n\
         \x20           residual: weighted_least_squares\n\
         \x20           method levenberg_marquardt\n\
         \x20           initial: k = 1.0\n\
         \x20           data: t = [1.0, 2.0]\n\
         \x20           data: y = [2.0, 3.0]\n",
    )
    .expect("write negative fixture to temp dir");
    let (envelope, exit) = fit_json_output(&[path.to_str().expect("utf8 temp path")]);
    p.eq("1", exit, emath_cli::CliExit::Refused);
    p.eq("2", json_bool(&envelope, "admitted"), Some(false));
    let diagnostics =
        json_array(&envelope, "diagnostics").expect("refusal envelope carries diagnostics");
    p.demand("3",diagnostics
            .iter()
            .any(|entry| json_str(entry, "code").is_some_and(|code| code == "E-FIT-004")), format!(
        "the refusal must name the missing model declaration"
    ));
    if p.failures().len() != f0 { return; }
    let _ = std::fs::remove_file(&path);

    });
    probe.finish();
}
