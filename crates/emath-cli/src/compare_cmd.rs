//! `emath compare`: evaluate one function under several carrier worlds
//! and report every answer side by side.
//!
//! A world is a configuration of the reference engine (see
//! `constructor_layer::world`): `reference` as authored, `exact` with
//! every real an exact rational, `float64` / `float32` / `bfloat16` with
//! real data rounded to that float format, `int64` with integer
//! arithmetic confined to signed 64-bit.
//! The command never picks a winner. It prints each world's answer or
//! refusal, the exact gap of each numeric answer from the first answer,
//! and a verdict: `agree` (every world answered the same value),
//! `disagree` (answers differ), or `partial` (some worlds refused and the
//! answers that exist agree). Disagreement is a finding, not a failure,
//! so the exit status is 0 whenever every world was evaluated.

use std::collections::BTreeMap;
use std::path::PathBuf;

use emath_artifact::JsonWriter;
use emath_core::tree::{Item, StmtKind, TypeExpr};
use emath_exec_ir::constructor_layer::{
    CValue, CarrierWorld, DEFAULT_WORK, answer_gap, answers_agree, evaluate_function_carrying,
    prepare_module, with_world, world_input,
};

use crate::{CliExit, EXIT_IO, EXIT_OK, EXIT_REFUSED, EXIT_USAGE};

pub(crate) const USAGE: &str = "compare <file.emath> --function F [--set name=value] [--world reference|exact|float64|float32|bfloat16|int64] [--work N] [--json]";

#[derive(Clone, Debug)]
pub(crate) struct CompareRequest {
    pub path: PathBuf,
    pub function: String,
    pub given: Vec<(String, String)>,
    pub worlds: Vec<String>,
    pub work: u64,
    pub json: bool,
}

impl CompareRequest {
    pub(crate) fn parse(rest: &[String]) -> Option<Self> {
        let (mut path, mut function) = (None, None);
        let mut given = Vec::new();
        let mut worlds = Vec::new();
        let mut work = DEFAULT_WORK;
        let mut json = false;
        let mut i = 0;
        while i < rest.len() {
            match rest[i].as_str() {
                "--function" => {
                    i += 1;
                    function = Some(rest.get(i)?.clone());
                }
                "--set" => {
                    i += 1;
                    let (k, v) = rest.get(i)?.split_once('=')?;
                    given.push((k.to_string(), v.to_string()));
                }
                "--world" => {
                    i += 1;
                    worlds.push(rest.get(i)?.clone());
                }
                "--work" => {
                    i += 1;
                    work = rest.get(i)?.parse().ok()?;
                }
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
        Some(Self {
            path: path?,
            function: function?,
            given,
            worlds,
            work,
            json,
        })
    }
}

struct Outcome {
    world: CarrierWorld,
    inputs: BTreeMap<String, CValue>,
    answer: Result<CValue, (String, String)>,
}

/// What every returned answer asserts, in the form an independent
/// checker consumes: this function of this source, under this language
/// image and world, applied to these inputs (as the world read them),
/// returned this value. emath does not check it; `status` stays
/// `unchecked` until a checker answers for `id`.
struct Claim {
    id: String,
    statement: String,
}

struct Identity {
    language_id: String,
    source_id: String,
}

fn claim(identity: &Identity, function: &str, outcome: &Outcome, answer: &CValue) -> Claim {
    let inputs: Vec<String> = outcome
        .inputs
        .iter()
        .map(|(name, value)| format!("{name} = {value}"))
        .collect();
    let statement = format!(
        "{function}({}) = {answer} in world {}",
        inputs.join(", "),
        outcome.world.name()
    );
    // Canonical text: one field per line, inputs in name order (the map
    // is ordered), so the id is reproducible across runs and hosts.
    let canonical = format!(
        "emath.claim.v1\nlanguage={}\nsource={}\nfunction={function}\ninputs={}\nworld={}\nanswer={answer}\n",
        identity.language_id,
        identity.source_id,
        inputs.join("; "),
        outcome.world.name(),
    );
    Claim {
        id: emath_core::content_id_of_str(&canonical).0,
        statement,
    }
}

fn refuse(json: bool, code: &str, message: &str, exit: CliExit) -> CliExit {
    if json {
        let mut out = JsonWriter::object();
        out.string("schema_version", "emath.compare.v1");
        out.string("command", "compare");
        out.string("code", code);
        out.string("message", message);
        println!("{}", out.finish());
    } else {
        eprintln!("error {code}: {message}");
    }
    exit
}

/// Declared input types of the named entry in the main file.
fn declared_inputs(
    tree: &emath_core::tree::SyntaxTree,
    function: &str,
) -> BTreeMap<String, TypeExpr> {
    let mut types = BTreeMap::new();
    for item in &tree.items {
        let Item::Declaration(decl) = item else {
            continue;
        };
        if decl.name != function {
            continue;
        }
        for section in decl.sections().filter(|s| s.name == "inputs") {
            for stmt in &section.suite.statements {
                if let StmtKind::FieldDecl { name, ty, .. } = &stmt.kind {
                    types.insert(name.clone(), ty.clone());
                }
            }
        }
    }
    types
}

fn representation(value: &CValue) -> &'static str {
    match value {
        CValue::Int(_) | CValue::Rat { .. } | CValue::Bool(_) => "exact_scalar",
        CValue::Float64(_) => "rounded_scalar",
        CValue::Code(_) => "code",
        _ => "structured",
    }
}

pub(crate) fn run(request: CompareRequest) -> CliExit {
    let mut worlds = Vec::new();
    for name in &request.worlds {
        match CarrierWorld::parse(name) {
            Some(world) if !worlds.contains(&world) => worlds.push(world),
            Some(_) => {}
            None => {
                return refuse(
                    request.json,
                    "E-CLI-USAGE",
                    &format!("unknown world `{name}`; worlds are reference, exact, float64, float32, bfloat16, int64"),
                    EXIT_USAGE,
                );
            }
        }
    }
    if worlds.is_empty() {
        worlds = CarrierWorld::ALL.to_vec();
    }
    let source = match std::fs::read_to_string(&request.path) {
        Ok(text) => text,
        Err(err) => {
            return refuse(
                request.json,
                "E-CLI-IO",
                &format!("cannot read {}: {err}", request.path.display()),
                EXIT_IO,
            );
        }
    };
    let language_id = match crate::cli_dispatch::load_verified_language(Some(&request.path)) {
        Ok(distribution) => distribution.image.distribution_hash.to_string(),
        Err(detail) => return refuse(request.json, "E-LANG-IMAGE", &detail, EXIT_REFUSED),
    };
    let identity = Identity {
        language_id,
        source_id: emath_core::content_id_of_str(&source).0,
    };
    let (tree, diagnostics) = emath_syntax::parse_str(&source);
    if diagnostics.has_errors() {
        crate::print_diagnostics(&diagnostics);
        return EXIT_REFUSED;
    }
    let declared = declared_inputs(&tree, &request.function);
    let outcomes: Vec<Outcome> = worlds
        .iter()
        .map(|&world| {
            let inputs: BTreeMap<String, CValue> = request
                .given
                .iter()
                .map(|(name, raw)| (name.clone(), world_input(world, declared.get(name), raw)))
                .collect();
            let answer = with_world(world, || {
                prepare_module(&tree, Some(&request.path))
                    .and_then(|module| {
                        evaluate_function_carrying(
                            &module,
                            &request.function,
                            &inputs,
                            &mut BTreeMap::new(),
                            request.work,
                        )
                    })
                    .and_then(|run| run.value)
            })
            .map_err(|err| (err.code, err.message));
            Outcome {
                world,
                inputs,
                answer,
            }
        })
        .collect();
    report(&request, &identity, &outcomes)
}

fn report(request: &CompareRequest, identity: &Identity, outcomes: &[Outcome]) -> CliExit {
    let baseline = outcomes.iter().find_map(|o| o.answer.as_ref().ok());
    let mut differ = false;
    let mut refused = false;
    let mut rows = Vec::new();
    let mut lines = Vec::new();
    for outcome in outcomes {
        let mut row = JsonWriter::object();
        row.string("world", outcome.world.name());
        match &outcome.answer {
            Ok(value) => {
                let base = baseline.expect("a returned answer implies a baseline");
                // A comparison that cannot be made (a buffer) counts as a
                // difference: the worlds were not shown to agree.
                if !answers_agree(value, base).unwrap_or(false) {
                    differ = true;
                }
                let gap = answer_gap(value, base);
                row.string("execution", "returned");
                row.string("representation", representation(value));
                row.string("payload", &value.to_string());
                let mut line = format!("{:<10} {}", outcome.world.name(), value);
                if let Some(gap) = gap.filter(|g| *g != 0.0) {
                    row.field("gap", &format!("{gap:e}"));
                    line.push_str(&format!("   (gap {gap:e})"));
                }
                let claim = claim(identity, &request.function, outcome, value);
                let mut claim_row = JsonWriter::object();
                claim_row.string("id", &claim.id);
                claim_row.string("statement", &claim.statement);
                claim_row.string("status", "unchecked");
                row.object_field("claim", &claim_row.finish());
                lines.push(line);
            }
            Err((code, message)) => {
                refused = true;
                row.string("execution", "refused");
                row.string("code", code);
                row.string("message", message);
                lines.push(format!(
                    "{:<10} refused {code}: {message}",
                    outcome.world.name()
                ));
            }
        }
        rows.push(row.finish());
    }
    let verdict = match (differ, refused) {
        (true, _) => "disagree",
        (false, true) => "partial",
        (false, false) => "agree",
    };
    if request.json {
        let mut out = JsonWriter::object();
        out.string("schema_version", "emath.compare.v1");
        out.string("command", "compare");
        out.string("function", &request.function);
        out.string("language_id", &identity.language_id);
        out.string("source_id", &identity.source_id);
        out.objects("worlds", &rows);
        out.string("verdict", verdict);
        println!("{}", out.finish());
    } else {
        lines.iter().for_each(|line| println!("{line}"));
        println!("verdict: {verdict}");
    }
    EXIT_OK
}
