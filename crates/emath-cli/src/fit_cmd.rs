//! `emath fit`: execute a declared fit goal (04 §5.3,
//! emath-r3-fit-goal-4xjh) to fitted values with linked provenance.
//!
//! The fit program — parameters, observable, model path, prediction
//! label, residual method, optimizer method, seeds, weights, data rows,
//! and the identifiability honesty gate — is plain `.emath` data
//! carried by the elaborated goal payload. This command parses, admits,
//! plans, traces the payload into the generic runtime goal
//! (`emath_calibration::FitGoal::from_payload`), evaluates the declared
//! `emath model` through the interpreter (the model math stays in
//! `.emath`; no domain math exists in Rust), runs the generic
//! Levenberg-Marquardt fit with the executable numeric rank oracle, and
//! emits fitted values with linked `Fitted` provenance.

use super::{
    json_diagnostic_entry, json_diagnostics_entries, print_diagnostics, print_json_diagnostics,
    split_error_code, CliExit, EXIT_OK, EXIT_REFUSED,
};
use emath_artifact::JsonWriter;
use emath_calibration::{
    FitGoal, FitModel, FitOutcome, NumericRankOracle, ProvenanceHash, materialize_measured,
};
use emath_core::limits::Limits;
use emath_exec_ir::interp::{Value, evaluate, format_f64};
use emath_exec_ir::{EmirProgram, definition_order, lower_definition};
use emath_ir::provenance::Provenance;
use emath_ir::{Declaration, SemanticPackage};
use emath_sema::session::CompilerSession;
use emath_term::SymbolId;
use std::collections::BTreeMap;
use std::path::PathBuf;

pub(crate) fn dispatch_fit(args: &FitArgs) -> CliExit {
    fit_cmd(args)
}

pub(crate) struct FitArgs {
    path: PathBuf,
    json: bool,
}

pub(crate) fn parse_fit_args(args: &[String]) -> Result<FitArgs, String> {
    let mut path = None;
    let mut json = false;
    for arg in args {
        match arg.as_str() {
            "--json" => json = true,
            other if other.starts_with('-') && other != "-" => {
                return Err(format!("unknown flag `{other}`"));
            }
            other if path.is_some() => {
                return Err(format!("unexpected extra argument `{other}`"));
            }
            other => path = Some(PathBuf::from(other)),
        }
    }
    Ok(FitArgs {
        path: path.ok_or_else(|| "missing <file.emath>".to_string())?,
        json,
    })
}

fn refuse_fit(text: &str, json: bool) -> CliExit {
    eprintln!("error: {text}");
    if json {
        let (code, message) = split_error_code(text).unwrap_or(("E-FIT-000", text));
        let mut out = JsonWriter::object();
        out.string("command", "fit");
        out.bool("admitted", false);
        out.objects(
            "diagnostics",
            &[json_diagnostic_entry(code, "error", message)],
        );
        print!("{}", out.finish());
    }
    EXIT_REFUSED
}

fn fit_cmd(args: &FitArgs) -> CliExit {
    let mut session = CompilerSession::new(Limits::default());
    let Ok(package) = session.load_package(&args.path) else {
        return refuse_fit(
            &format!(
                "E-PKG-080: cannot read source file ({})",
                args.path.display()
            ),
            args.json,
        );
    };
    let checked = session.check(package.file);
    if checked.diagnostics.has_errors() {
        print_diagnostics(&checked.diagnostics);
        if args.json {
            print_json_diagnostics("fit", false, &json_diagnostics_entries(&checked.diagnostics));
        }
        return EXIT_REFUSED;
    }
    let planned = session.plan(package.file);
    if planned.diagnostics.has_errors() {
        print_diagnostics(&planned.diagnostics);
        if args.json {
            print_json_diagnostics("fit", false, &json_diagnostics_entries(&planned.diagnostics));
        }
        return EXIT_REFUSED;
    }
    let Some(request) = planned
        .requests
        .iter()
        .find(|request| request.kind == "fit")
    else {
        return refuse_fit(
            &format!(
                "E-FIT-002: {} declares no fit goal (`fit <params> to <observable>:`)",
                args.path.display()
            ),
            args.json,
        );
    };
    let goal = match FitGoal::from_payload(&request.payload, &request.target) {
        Ok(goal) => goal,
        Err(error) => {
            return refuse_fit(
                &format!("E-FIT-003: fit payload refused: {error:?}"),
                args.json,
            );
        }
    };
    let model_name = goal.model.last().cloned().unwrap_or_default();
    let Some(declaration) = checked
        .package
        .declarations
        .iter()
        .find(|declaration| {
            declaration.kind_label == "model" && declaration.name.leaf() == model_name
        })
    else {
        return refuse_fit(
            &format!(
                "E-FIT-004: fit names model `{}` but `{}` declares no `emath model {model_name}`",
                goal.model.join("::"),
                args.path.display()
            ),
            args.json,
        );
    };
    // The model must be evaluable from the fit vocabulary: every input
    // is either a fitted parameter or the data coordinate.
    for field in &declaration.inputs {
        let is_parameter = goal.parameters.iter().any(|symbol| symbol.0 == field.name);
        if !is_parameter && field.name != goal.coordinate {
            return refuse_fit(
                &format!(
                    "E-FIT-005: model input `{}` is neither a fitted parameter \
                     nor the data coordinate `{}`",
                    field.name, goal.coordinate
                ),
                args.json,
            );
        }
    }
    if goal.prediction.is_empty() {
        return refuse_fit(
            "E-FIT-006: the fit goal must name a prediction label (`prediction <label>`)",
            args.json,
        );
    }
    // Lowering the model's definitions happens ONCE here (they depend
    // only on the package + declaration, never on parameter values);
    // construction errors surface with the same E-FIT-012 refusal the
    // per-call path produced.
    let model = match PackageModel::new(
        &checked.package,
        declaration,
        goal.prediction.clone(),
        goal.coordinate.clone(),
    ) {
        Ok(model) => model,
        Err(detail) => {
            return refuse_fit(
                &format!("E-FIT-012: model evaluation failed: {detail}"),
                args.json,
            );
        }
    };
    let oracle = NumericRankOracle::default();
    let outcome = emath_calibration::fit(&goal, &model, &goal.data, Some(&oracle));
    match outcome {
        FitOutcome::Fitted {
            parameters,
            hash,
            confidence,
        } => {
            emit_fitted(&args, &goal, &parameters, hash, confidence.as_ref());
            EXIT_OK
        }
        FitOutcome::AuthorityRefused { direction, reason } => refuse_fit(
            &format!(
                "E-FIT-010: identifiability escalation refused for direction `{}` ({:?})",
                direction.0, reason
            ),
            args.json,
        ),
        FitOutcome::Unresolved { reason } => {
            refuse_fit(&format!("E-FIT-011: fit unresolved ({reason:?})"), args.json)
        }
        FitOutcome::ModelError { detail } => {
            refuse_fit(&format!("E-FIT-012: model evaluation failed: {detail}"), args.json)
        }
    }
}

/// Interpreter-backed model seam: evaluates the declared `emath model`
/// over the fit vocabulary. All model math lives in the `.emath`
/// declaration — nothing domain-specific exists in Rust.
///
/// The definition programs are lowered ONCE at construction: they depend
/// only on the package, the declaration, and the input names — never on
/// parameter values — while `predict` runs O(iterations · rows ·
/// (1 + 2·parameters)) times over the fit. Lowering per call re-did that
/// whole walk every evaluation.
struct PackageModel {
    prediction: String,
    coordinate: String,
    /// Input names in declaration order (the evaluation stack prefix).
    input_names: Vec<String>,
    /// `(definition name, lowered program)` in evaluation order.
    programs: Vec<(String, EmirProgram)>,
    /// Index of the prediction label in the evaluation stack.
    prediction_index: usize,
}

impl PackageModel {
    /// Lowers the model's definitions once, in evaluation order. The
    /// error texts match the per-call failures this constructor replaces
    /// (lowering faults, missing prediction label) so the CLI refusal
    /// is unchanged.
    fn new(
        package: &SemanticPackage,
        declaration: &Declaration,
        prediction: String,
        coordinate: String,
    ) -> Result<Self, String> {
        let input_names: Vec<String> = declaration
            .inputs
            .iter()
            .map(|field| field.name.clone())
            .collect();
        let mut seen = input_names.clone();
        let mut programs = Vec::new();
        for (definition_name, expr) in definition_order(package, declaration) {
            let program = lower_definition(package, expr, &seen, &[]).map_err(|detail| {
                format!("lowering definition `{definition_name}`: {detail}")
            })?;
            seen.push(definition_name.clone());
            programs.push((definition_name.clone(), program));
        }
        let prediction_index = seen
            .iter()
            .position(|name| name == &prediction)
            .ok_or_else(|| {
                format!(
                    "prediction label `{}` is not a defined output of model `{}`",
                    prediction,
                    declaration.name.leaf()
                )
            })?;
        Ok(Self {
            prediction,
            coordinate,
            input_names,
            programs,
            prediction_index,
        })
    }
}

impl FitModel for PackageModel {
    fn predict(&self, parameters: &BTreeMap<SymbolId, f64>, t: f64) -> Result<f64, String> {
        let mut stack: Vec<Value> =
            Vec::with_capacity(self.input_names.len() + self.programs.len());
        for name in &self.input_names {
            let value = if *name == self.coordinate {
                t
            } else {
                let symbol = SymbolId(name.clone());
                *parameters.get(&symbol).ok_or_else(|| {
                    format!("model input `{}` is not bound by the fit", name)
                })?
            };
            stack.push(Value::F64(value));
        }
        for (definition_name, program) in &self.programs {
            let value = evaluate(program, &stack, &[]).map_err(|fault| {
                format!("evaluating definition `{definition_name}`: {fault:?}")
            })?;
            stack.push(value);
        }
        stack[self.prediction_index].as_real_f64().ok_or_else(|| {
            format!("prediction `{}` is not a Float64 value", self.prediction)
        })
    }
}

fn emit_fitted(
    args: &FitArgs,
    goal: &FitGoal,
    parameters: &BTreeMap<SymbolId, f64>,
    hash: ProvenanceHash,
    confidence: Option<&emath_calibration::Identifiability>,
) {
    let hash_hex = format!("{:016x}", hash.0);
    let measured = materialize_measured(goal, parameters, hash, confidence);
    if args.json {
        let mut out = JsonWriter::object();
        out.string("command", "fit");
        out.string("file", &args.path.display().to_string());
        out.string("model", &goal.model.join("::"));
        out.string("prediction", &goal.prediction);
        out.string("residual", goal.residual.as_str());
        out.string("method", goal.method.as_str());
        out.bool("admitted", true);
        out.string("hash", &hash_hex);
        let mut parameter_object = JsonWriter::object();
        for parameter in &goal.parameters {
            let value = parameters.get(parameter).copied().unwrap_or(0.0);
            parameter_object.field(&parameter.0, &format_f64(value));
        }
        out.object_field("parameters", &parameter_object.finish());
        let mut confidence_rows = Vec::new();
        if let Some(verdict) = confidence {
            for (symbol, interval) in &verdict.directions {
                let mut row = JsonWriter::object();
                row.string("direction", &symbol.0);
                row.field("lo", &format_f64(interval.lo));
                row.field("hi", &format_f64(interval.hi));
                row.bool("tight", interval.tight);
                confidence_rows.push(row.finish());
            }
        }
        out.objects("confidence", &confidence_rows);
        let mut measured_rows = Vec::new();
        if let Ok(measured) = measured {
            for (symbol, value) in measured {
                let mut row = JsonWriter::object();
                row.string("name", &symbol.0);
                row.field("value", &format_f64(value.value));
                row.field("std_uncertainty", &format_f64(value.std_uncertainty));
                row.string("distribution", &format!("{:?}", value.distribution));
                let mut provenance_object = JsonWriter::object();
                match &value.provenance {
                    Provenance::Fitted { fit_id } => {
                        provenance_object.string("kind", "Fitted");
                        provenance_object.string("fit_id", fit_id);
                    }
                    other => {
                        provenance_object.string("kind", other.variant_name());
                    }
                }
                row.object_field("provenance", &provenance_object.finish());
                measured_rows.push(row.finish());
            }
        }
        out.objects("measured", &measured_rows);
        out.field("data_rows", &goal.data.len().to_string());
        print!("{}", out.finish());
        return;
    }
    println!(
        "fit {} model={} prediction={} residual={} method={} data_rows={}",
        args.path.display(),
        goal.model.join("::"),
        goal.prediction,
        goal.residual.as_str(),
        goal.method.as_str(),
        goal.data.len()
    );
    for parameter in &goal.parameters {
        let value = parameters.get(parameter).copied().unwrap_or(0.0);
        println!("fitted {} = {}", parameter.0, value);
    }
    println!("hash = {}", hash_hex);
    if let Some(verdict) = confidence {
        for (symbol, interval) in &verdict.directions {
            println!(
                "confidence {} = [{} .. {}] tight={}",
                symbol.0, interval.lo, interval.hi, interval.tight
            );
        }
    }
    if let Ok(measured) = measured {
        for (symbol, value) in measured {
            println!(
                "measured {} = {} +/- {} (provenance Fitted[{}])",
                symbol.0,
                value.value,
                value.std_uncertainty,
                hash_hex
            );
        }
    }
}
