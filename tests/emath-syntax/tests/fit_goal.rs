//! Fit goal surface (bead emath-r3-fit-goal-4xjh, 04 §5.3) — generic
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

fn install_source_parser() {
    emath_syntax::install_source_parser();
}

fn check(text: &str, name: &str) -> Vec<String> {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let result = session.check_owned(name, text);
    result
        .diagnostics
        .errors()
        .map(|d| format!("{} {}", d.code, d.message))
        .collect()
}

fn plan_requests(text: &str, name: &str) -> (Vec<String>, Vec<(String, String)>) {
    install_source_parser();
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
    install_source_parser();
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

#[test]
fn pk_fit_fixture_admits_and_lowers_to_fit_request() {
    let errors = check(PK_FIT_FIXTURE, "pk-two-compartment-fit");
    assert!(
        errors.is_empty(),
        "the runnable PK fit fixture must admit with zero errors; got: {errors:#?}"
    );
}

#[test]
fn fit_request_carries_full_generic_program_payload() {
    let (diagnostics, payload) = plan_fit_payload(PK_FIT_FIXTURE, "pk-two-compartment-fit");
    assert!(
        diagnostics.is_empty(),
        "the PK fit fixture must plan with zero errors; got: {diagnostics:#?}"
    );
    let payload = payload.expect("the fit request must carry its payload");
    assert_eq!(
        payload.parameters, vec!["k_el", "V_central"],
        "parameters must survive in declared order"
    );
    assert_eq!(payload.model, vec!["PkTwoCompartment"]);
    assert_eq!(payload.prediction, "central");
    assert_eq!(payload.residual, "weighted_least_squares");
    assert_eq!(payload.method, "levenberg_marquardt");
    assert_eq!(
        payload.initial,
        vec![("k_el".to_string(), "0.2".to_string()), ("V_central".to_string(), "1.0".to_string())],
        "seed literals must survive losslessly"
    );
    assert_eq!(payload.weights.len(), 2, "explicit weights must survive");
    assert_eq!(
        payload.data,
        vec![
            (
                "t".to_string(),
                vec!["0.5".to_string(), "1.0".to_string(), "2.0".to_string(), "4.0".to_string()]
            ),
            (
                "conc_time".to_string(),
                vec!["3.12".to_string(), "2.43".to_string(), "1.47".to_string(), "0.54".to_string()]
            ),
        ],
        "declared data rows must survive losslessly, one naming the observable"
    );
    assert!(
        payload.require_identifiability,
        "the honesty gate must survive lowering"
    );
}

#[test]
fn single_param_fit_row_admits() {
    let errors = check(FIT_SINGLE, "fit-single");
    assert!(
        errors.is_empty(),
        "a minimal fit goal must admit; got: {errors:#?}"
    );
}

#[test]
fn fit_requires_residual_row_at_lowering() {
    let (diagnostics, _) = plan_requests(FIT_MISSING_RESIDUAL, "fit-missing-residual");
    assert!(
        diagnostics
            .iter()
            .any(|d| d.starts_with("E-GOAL-042") && d.contains("residual")),
        "a fit without an explicit residual row must refuse at lowering; got: {diagnostics:#?}"
    );
}

#[test]
fn malformed_fit_row_names_the_missing_to() {
    let errors = check(FIT_MALFORMED_MISSING_TO, "fit-malformed");
    assert!(
        errors
            .iter()
            .any(|e| e.starts_with("E-SYN-101") && e.contains("to")),
        "a malformed fit row must name the expected `to`: got: {errors:#?}"
    );
    assert!(
        !errors
            .iter()
            .any(|e| e.contains("design follow-up") || e.contains("outside the Phase 1 subset")),
        "the old design-fence refusal must be gone; got: {errors:#?}"
    );
}

#[test]
fn plain_functions_still_admit() {
    let errors = check(PLAIN_GOALS, "fit-plain-guard");
    assert!(
        errors.is_empty(),
        "the fit grammar must not affect ordinary functions; got: {errors:#?}"
    );
}

#[test]
fn pk_fit_fixture_is_byte_canonical_under_the_formatter() {
    install_source_parser();
    let parsed = emath_syntax::parse_lossless(
        PK_FIT_FIXTURE,
        emath_core::FileId(0),
        &Limits::default(),
    );
    assert!(
        !parsed.diagnostics.has_errors(),
        "fixture must parse before formatting"
    );
    let canonical = emath_syntax::formatter::format(&parsed.tree, &parsed.comments);
    assert_eq!(
        canonical, PK_FIT_FIXTURE,
        "the fit fixture must be byte-canonical (fmt(file) == file)"
    );
}

#[test]
fn fit_plan_excludes_missing_identifiability_provider() {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let file = session.load_text("pk-two-compartment-fit", PK_FIT_FIXTURE);
    let result = session.plan(file);
    let excluded = result
        .plans
        .iter()
        .flat_map(|plan| plan.excluded_candidates.iter())
        .filter(|candidate| candidate.provider == "fit.structural-identifiability")
        .collect::<Vec<_>>();
    assert!(
        !excluded.is_empty(),
        "every fit plan must exclude the missing structural-identifiability provider"
    );
    assert!(
        excluded
            .iter()
            .all(|candidate| candidate.reason.contains("unresolved")),
        "the exclusion must carry the honest unresolved disposition"
    );
}

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
        JsonValue::Obj(entries) => entries
            .iter()
            .find(|(key, _)| key == name)
            .and_then(|(_, value)| match value {
                JsonValue::Str(text) => Some(text),
                _ => None,
            }),
        _ => None,
    }
}

fn json_bool(value: &JsonValue, name: &str) -> Option<bool> {
    match value {
        JsonValue::Obj(entries) => entries
            .iter()
            .find(|(key, _)| key == name)
            .and_then(|(_, value)| match value {
                JsonValue::Bool(flag) => Some(*flag),
                _ => None,
            }),
        _ => None,
    }
}

fn json_array<'a>(value: &'a JsonValue, name: &str) -> Option<&'a Vec<JsonValue>> {
    match value {
        JsonValue::Obj(entries) => entries
            .iter()
            .find(|(key, _)| key == name)
            .and_then(|(_, value)| match value {
                JsonValue::Arr(items) => Some(items),
                _ => None,
            }),
        _ => None,
    }
}

#[test]
fn cli_fit_executes_pk_fixture_to_fitted_values_with_linked_provenance() {
    let fixture = "../../language/examples/science/pk-two-compartment-fit.emath";
    let (envelope, exit) = fit_json_output(&[fixture]);
    assert_eq!(exit, emath_cli::CliExit::Ok, "the fixture fit must exit 0");
    assert_eq!(
        json_str(&envelope, "command"),
        Some(&"fit".to_string()),
        "the envelope must name the command"
    );
    assert_eq!(json_bool(&envelope, "admitted"), Some(true));
    assert_eq!(
        json_str(&envelope, "model"),
        Some(&"PkTwoCompartment".to_string())
    );
    assert_eq!(json_str(&envelope, "prediction"), Some(&"central".to_string()));
    assert_eq!(
        json_str(&envelope, "residual"),
        Some(&"weighted_least_squares".to_string())
    );
    assert_eq!(
        json_str(&envelope, "method"),
        Some(&"levenberg_marquardt".to_string())
    );
    let hash = json_str(&envelope, "hash")
        .expect("fitted envelope carries the content hash")
        .clone();
    assert_eq!(hash.len(), 16, "the hash must be 16 hex digits");
    let (measured_k_el, measured_v) = fitted_numbers(&envelope);
    assert!(
        (measured_k_el - 0.5).abs() < 0.05 && (measured_v - 25.0).abs() < 3.0,
        "the fixture data lies on C(t) = 100/V * exp(-k t) with k=0.5, V=25; \
         got k_el={measured_k_el}, V_central={measured_v}"
    );
    let confidence = json_array(&envelope, "confidence")
        .expect("granted fit carries confidence rows");
    assert_eq!(confidence.len(), 2, "one interval per declared direction");
    assert!(
        confidence.iter().all(|row| json_bool(row, "tight") == Some(true)),
        "full-rank fixture data must certify every direction tight"
    );
    let measured = json_array(&envelope, "measured")
        .expect("fitted envelope carries materialized measured values");
    assert_eq!(measured.len(), 2, "one measured value per fitted parameter");
    for row in measured {
        let Some(fit_id) = provenance_fit_id(row) else {
            panic!("measured row must carry Fitted provenance with fit_id");
        };
        assert_eq!(fit_id, hash, "the provenance fit_id must be the envelope hash");
    }
    // Determinism: the same program + data + seed + method hashes
    // identically across runs.
    let (second, second_exit) = fit_json_output(&[fixture]);
    assert_eq!(second_exit, emath_cli::CliExit::Ok);
    assert_eq!(
        json_str(&second, "hash"),
        Some(&hash),
        "identical fit programs must hash identically (determinism class)"
    );
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

#[test]
fn cli_fit_refuses_a_fit_without_a_model_declaration() {
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
    assert_eq!(
        exit,
        emath_cli::CliExit::Refused,
        "a fit naming an undeclared model must refuse"
    );
    assert_eq!(json_bool(&envelope, "admitted"), Some(false));
    let diagnostics = json_array(&envelope, "diagnostics")
        .expect("refusal envelope carries diagnostics");
    assert!(
        diagnostics
            .iter()
            .any(|entry| json_str(entry, "code").is_some_and(|code| code == "E-FIT-004")),
        "the refusal must name the missing model declaration"
    );
    let _ = std::fs::remove_file(&path);
}
