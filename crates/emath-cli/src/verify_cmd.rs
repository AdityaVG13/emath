//! `emath verify`: recheck every kept claim in a library file against
//! the checkers the library itself names. For each own function whose
//! declared output is the claim type, verify evaluates the entry (the
//! keep shape: one `unused: Int` input), builds a registry entry for
//! each distinct named checker that resolves in the library's own
//! scope, and runs the frozen `check_claim` on the stored certificate
//! — the search that produced the claim is never rerun. One row per
//! claim; any verdict other than accepted refuses with the claim
//! named; `--json` emits the same rows.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use emath_core::json_escape;
use emath_core::tree::{Expr, ExprKind, Item};
use emath_exec_ir::constructor_layer::{
    parse_constructor_scalar, CValue, DEFAULT_WORK, PreparedModule,
};
use emath_syntax::formatter::{format_expr, Prec};

use crate::{CliExit, EXIT_ADMISSION, EXIT_FAULT, EXIT_IO, EXIT_OK, EXIT_REFUSED};

pub(crate) const USAGE: &str = "verify <library.emath> [--json]";

#[derive(Clone, Debug)]
pub(crate) struct VerifyRequest {
    pub path: PathBuf,
    pub json: bool,
}

impl VerifyRequest {
    pub(crate) fn parse(rest: &[String]) -> Option<Self> {
        let (mut path, mut json) = (None, false);
        let mut i = 0;
        while i < rest.len() {
            match rest[i].as_str() {
                "--json" => json = true,
                other => {
                    if other.starts_with('-') || path.is_some() {
                        return None;
                    }
                    path = Some(PathBuf::from(other));
                }
            }
            i += 1;
        }
        Some(Self { path: path?, json })
    }
}

/// One claim's recheck outcome. `kind`/`standing` are the claim's own
/// closed-set tags carried as data; `checker`/`conditions` are the
/// stored code rendered as text.
struct ClaimRow {
    name: String,
    kind: String,
    standing: String,
    checker: String,
    conditions: String,
    verdict: &'static str,
}

fn refuse(code: &str, message: impl std::fmt::Display, exit: CliExit) -> CliExit {
    eprintln!("error {code}: {message}");
    exit
}

/// The code's surface text (a checker name, a condition program).
fn code_text(code: &emath_exec_ir::constructor_layer::Code) -> String {
    let mut text = String::new();
    format_expr(&mut text, &code.expr, Prec::Root);
    text
}

/// Every declaration name in the module's full scope (own plus the
/// retained import graph): a claim's checker resolves when its name
/// appears here.
fn declared_names(module: &PreparedModule) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    if let Ok(tree) = module.merged_tree() {
        for item in &tree.items {
            if let Item::Declaration(decl) = item {
                names.insert(decl.name.clone());
            }
        }
    }
    names
}

fn path_expr(name: &str) -> Expr {
    Expr {
        kind: ExprKind::Path { segments: vec![name.to_string()], generics: None },
        source: emath_core::Span::default(),
    }
}

fn quoted(expr: Expr) -> Expr {
    Expr {
        kind: ExprKind::Quote { body: Box::new(expr) },
        source: emath_core::Span::default(),
    }
}

pub(crate) fn run(request: VerifyRequest) -> CliExit {
    let checked = crate::cli_build::checked_file(&request.path);
    if let Some(first) = checked.result.diagnostics.errors().next() {
        if checked.store.files().is_empty() {
            return refuse("E-CLI-IO", &first.message, EXIT_IO);
        }
        eprintln!("{}", crate::cli_json::diagnostics_text(&checked.result.diagnostics, &checked.store));
        return EXIT_ADMISSION;
    }
    let Some(module) = &checked.result.prepared else {
        return refuse("E-VERIFY-SHAPE", "checking produced no executable module", EXIT_ADMISSION);
    };
    let library = request.path.display();
    let claims: Vec<String> = module
        .function_output_types()
        .into_iter()
        .filter(|(_, outputs)| outputs.iter().any(|name| name == "Claim"))
        .map(|(name, _)| name)
        .collect();

    // Registry entries are built per distinct named checker, only for
    // names that resolve in the library's scope: a claim naming a
    // stranger leaves its checker absent, and check_claim's own gate
    // answers the refusal.
    let declared = declared_names(module);
    let mut registry_names: Vec<String> = Vec::new();
    let mut rows: Vec<ClaimRow> = Vec::new();
    let mut refusal: Option<(&'static str, String)> = None;
    for name in &claims {
        let inputs: BTreeMap<String, CValue> =
            BTreeMap::from([("unused".to_string(), parse_constructor_scalar("0"))]);
        let value = match module.evaluate_function(name, &inputs, DEFAULT_WORK) {
            Ok(value) => value,
            Err(err) => return refuse(&err.code, format!("{library}: {name}: {}", err.message), EXIT_FAULT),
        };
        let CValue::Record { type_name, fields } = &value else {
            return refuse("E-VERIFY-SHAPE", format!("{library}: {name} declares a Claim output but evaluated to {value}"), EXIT_REFUSED);
        };
        if type_name.as_ref() != "Claim" {
            return refuse("E-VERIFY-SHAPE", format!("{library}: {name} declares a Claim output but evaluated to a {type_name}"), EXIT_REFUSED);
        }
        let (Some(CValue::Int(kind)), Some(CValue::Int(standing))) = (fields.get("kind"), fields.get("standing")) else {
            return refuse("E-VERIFY-SHAPE", format!("{library}: {name} is missing its kind or standing tag"), EXIT_REFUSED);
        };
        let (Some(CValue::Code(checker)), Some(CValue::Code(conditions))) = (fields.get("checker"), fields.get("conditions")) else {
            return refuse("E-VERIFY-SHAPE", format!("{library}: {name} is missing its checker or conditions code"), EXIT_REFUSED);
        };
        let checker_name = code_text(checker);
        if !registry_names.contains(&checker_name) && declared.contains(&checker_name) {
            registry_names.push(checker_name.clone());
        }

        let registry = Expr {
            kind: ExprKind::List(
                registry_names
                    .iter()
                    .map(|checker| Expr {
                        kind: ExprKind::Record {
                            type_path: vec!["Checker".to_string()],
                            fields: vec![
                                ("name".to_string(), quoted(path_expr(checker))),
                                ("run".to_string(), path_expr(checker)),
                            ],
                        },
                        source: emath_core::Span::default(),
                    })
                    .collect(),
            ),
            source: emath_core::Span::default(),
        };
        let call = Expr {
            kind: ExprKind::Call {
                function: Box::new(path_expr("check_claim")),
                args: vec![path_expr("claim"), registry],
            },
            source: emath_core::Span::default(),
        };
        let verdict = match module.evaluate_expr(
            call,
            BTreeMap::from([("claim".to_string(), value.clone())]),
        ) {
            Ok(verdict) => verdict,
            Err(err) => {
                return refuse(&err.code, format!("{library}: {name}: {}", err.message), EXIT_FAULT)
            }
        };
        let CValue::Record { fields: verdict_fields, .. } = &verdict else {
            return refuse("E-VERIFY-SHAPE", format!("{library}: {name}: check_claim did not return a verdict"), EXIT_REFUSED);
        };
        let Some(CValue::Int(status)) = verdict_fields.get("status") else {
            return refuse("E-VERIFY-SHAPE", format!("{library}: {name}: the verdict carries no status"), EXIT_REFUSED);
        };
        let row = ClaimRow {
            name: name.clone(),
            kind: kind.to_string(),
            standing: standing.to_string(),
            checker: checker_name.clone(),
            conditions: code_text(conditions),
            verdict: "accepted",
        };
        match status.to_i64() {
            Some(1) => rows.push(row),
            Some(0) => {
                refusal = Some(("E-VERIFY-REJECTED", format!("{library}: {name}: the checker {checker_name} rejected the stored certificate")));
                rows.push(ClaimRow { verdict: "refused", ..row });
                break;
            }
            Some(-1) => {
                refusal = Some(("E-VERIFY-CHECKER", format!("{library}: {name}: the claim names the checker `{checker_name}`, which this library does not define or use")));
                rows.push(ClaimRow { verdict: "refused", ..row });
                break;
            }
            Some(-2) => {
                refusal = Some(("E-VERIFY-SETS", format!("{library}: {name}: the claim's kind {kind} or standing {standing} is outside the claim's closed sets")));
                rows.push(ClaimRow { verdict: "refused", ..row });
                break;
            }
            Some(-3) => {
                refusal = Some(("E-VERIFY-CONTENT-ID", format!("{library}: {name}: the stored content id does not match the claim's content")));
                rows.push(ClaimRow { verdict: "refused", ..row });
                break;
            }
            other => {
                return refuse("E-VERIFY-SHAPE", format!("{library}: {name}: check_claim returned the unknown status {other:?}"), EXIT_REFUSED);
            }
        }
    }

    if request.json {
        let rows: Vec<String> = rows
            .iter()
            .map(|row| {
                format!(
                    "{{\"name\":\"{}\",\"kind\":{},\"standing\":{},\"checker\":\"{}\",\"conditions\":\"{}\",\"verdict\":\"{}\"}}",
                    json_escape(&row.name),
                    row.kind,
                    row.standing,
                    json_escape(&row.checker),
                    json_escape(&row.conditions),
                    row.verdict,
                )
            })
            .collect();
        println!("[{}]", rows.join(", "));
    } else {
        for row in &rows {
            println!(
                "{} kind={} standing={} checker={} conditions={} verdict={}",
                row.name, row.kind, row.standing, row.checker, row.conditions, row.verdict
            );
        }
    }
    match refusal {
        Some((code, message)) => refuse(code, message, EXIT_REFUSED),
        None => EXIT_OK,
    }
}
