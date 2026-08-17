//! Semantic Genesis CLI: `parse`, `signature`, `genesis`, `compile
//! --parametric`, `world show`, `portfolio show`.
//!
//! Pipeline: source bytes → glyphs → parse forest → signature inference →
//! Term IR → free world → world candidates → interpretation portfolio →
//! answer receipt → parametric Rust artifact. Emitted JSON is deterministic
//! and std-only.

use super::{EXIT_OK, EXIT_REFUSED, EXIT_USAGE};
use emath_core::limits::Limits;
use emath_genesis::{
    BooleanAlienWorld, Environment, FreeTermWorld, ModularAlienWorld, evaluate, forest,
    free_symbolic_world,
};
use emath_portfolio::{Authority, InterpretationCandidate, InterpretationPortfolio, ScoreVector};
use emath_syntax::genesis as genesis_syntax;
use emath_term::{Signature, Term, VariableId};
use emath_world_ir::{
    Fixity, MeaningOrigin, OperatorDef, OperatorSemantics, SymbolDef, WorldIr, fnv1a64,
};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

/// One analysis result reused by every subcommand.
pub struct Analysis {
    pub file: genesis_syntax::GenesisFile,
    pub parse_forest_json: String,
    pub parse_id: u64,
    pub inference: forest::SignatureInference,
    pub signature_json: String,
    pub signature_id: u64,
    pub term: Term,
    pub term_id: u64,
    pub source_hash: u64,
}

/// Reads, parses, and structurally analyzes a genesis source file.
pub fn analyze(path: &Path) -> Result<Analysis, String> {
    let source = fs::read_to_string(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let limits = Limits::default();
    let file = genesis_syntax::parse_genesis(&source, &limits).map_err(|errors| {
        let detail = errors
            .iter()
            .map(|error| format!("{}: {}", error.code, error.message))
            .collect::<Vec<_>>()
            .join("; ");
        format!("E-GEN-080: genesis parse refused: {detail}")
    })?;
    if file.body_text.is_empty() {
        return Err("E-GEN-081: genesis body expression is empty".into());
    }
    let forest_limits = forest::ForestLimits {
        max_nodes: 65_536,
        // `keep: pareto N` is a portfolio budget, never a parser cap: a
        // small budget used to throttle derivation retention and leave
        // the body unparseable (ambiguity 0). Parsing always runs at the
        // admission default.
        max_alternatives: 128,
        max_depth: 128,
    };
    let parse_forest =
        forest::build_forest_named(&file.body_text, &file.world_name, &forest_limits);
    if parse_forest.ambiguity_count() != 1 {
        return Err(format!(
            "E-GEN-082: reference body is not unique: ambiguity {}",
            parse_forest.ambiguity_count()
        ));
    }
    let term = parse_forest
        .unique_term()
        .map_err(|error| format!("{}: {}", error.code, error.message))?;
    let inference =
        forest::infer_signature_named(&file.body_text, &file.world_name, &forest_limits).map_err(
            |errors| {
                let detail = errors
                    .iter()
                    .map(|error| format!("{}: {}", error.code, error.message))
                    .collect::<Vec<_>>()
                    .join("; ");
                format!("E-GEN-083: signature inference refused: {detail}")
            },
        )?;
    if let Err(error) = inference.signature.validate(&term) {
        return Err(format!(
            "E-GEN-084: inferred signature rejects term: {error:?}"
        ));
    }
    let parse_id = parse_forest.parse_id();
    let signature_id = inference.signature_id();
    Ok(Analysis {
        source_hash: fnv1a64(source.as_bytes()),
        parse_forest_json: parse_forest.canonical_json(),
        parse_id,
        signature_json: inference.canonical_json(),
        signature_id,
        file,
        inference,
        term_id: fnv1a64(term.canonical().as_bytes()),
        term,
    })
}

fn write_if_requested(out: Option<&PathBuf>, name: &str, body: &str) -> Result<(), String> {
    if let Some(dir) = out {
        fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        let target = dir.join(name);
        fs::write(&target, body).map_err(|e| format!("cannot write {}: {e}", target.display()))?;
    }
    Ok(())
}

/// Emits single-line JSON for JSONL records (`world-admission.jsonl`).
fn jsonl(
    seq: u64,
    schema: &str,
    status: &str,
    code: &str,
    label: &str,
    world_id: Option<u64>,
) -> String {
    let id = world_id.map(|value| format!("\"world_id\":\"{value:016x}\""));
    let comma = if id.is_some() { "," } else { "" };
    format!(
        "{{\"seq\":{seq},\"schema\":\"{schema}\",\"status\":\"{status}\",\"code\":\"{code}\",\"label\":\"{label}\"{comma}{}}}",
        id.unwrap_or_default()
    )
}

/// Canonical declared-expression semantics for the built-in worlds.
fn declared_world(label: &str, signature: &Signature, semantics: &[(&str, &str)]) -> WorldIr {
    let symbols = signature
        .iter()
        .map(|(symbol, arity)| SymbolDef {
            id: symbol.clone(),
            display: symbol.0.clone(),
            fixity: if *arity == 0 {
                Fixity::Constant
            } else {
                Fixity::Function
            },
            precedence: None,
            type_scheme: format!("Term^{arity} -> Term"),
        })
        .collect::<Vec<_>>();
    let operators = semantics
        .iter()
        .map(|(symbol, meaning)| OperatorDef {
            symbol: emath_term::SymbolId((*symbol).into()),
            semantics: OperatorSemantics::DeclaredExpression((*meaning).into()),
            origin: MeaningOrigin::Declared,
        })
        .collect::<Vec<_>>();
    WorldIr {
        version: 1,
        name: label.into(),
        signature: signature.clone(),
        carriers: vec![emath_world_ir::CarrierDef {
            name: "Element".into(),
            type_expression: match label {
                "Boolean_algebra" => "Bool".into(),
                "modular_numeric" => "Z_17".into(),
                _ => "FreeTerm".into(),
            },
        }],
        symbols,
        operators,
        constructors: vec!["Element -> Constant/Apply".into()],
        laws: vec!["total".into(), "deterministic".into()],
        holes: vec![],
        capabilities: vec!["pure".into()],
    }
}

/// Admitted built-in world labels in this G0–G3 slice.
const ADMITTED_WORLDS: [&str; 3] = ["free_symbolic", "Boolean_algebra", "modular_numeric"];

/// Builds the three admitted `WorldIr` candidates for `signature`.
pub fn builtin_worlds(signature: &Signature) -> Vec<WorldIr> {
    let mut worlds = vec![free_symbolic_world("free_symbolic", signature.clone())];
    worlds.push(declared_world(
        "Boolean_algebra",
        signature,
        &[("ζ", "true"), ("⋈", "xor"), ("⧖", "not"), ("⊛", "and")],
    ));
    worlds.push(declared_world(
        "modular_numeric",
        signature,
        &[
            ("ζ", "3"),
            ("⋈", "(x+y) mod 17"),
            ("⧖", "(x*x) mod 17"),
            ("⊛", "(x*y) mod 17"),
        ],
    ));
    worlds
}

/// `parse <file> [--out <dir>]`: glyphs + bounded parse forest.
pub fn parse_cmd(path: &Path, out: Option<&PathBuf>, forest_only: bool) -> u8 {
    let analysis = match analyze(path) {
        Ok(analysis) => analysis,
        Err(error) => {
            eprintln!("error: {error}");
            return EXIT_REFUSED;
        }
    };
    if forest_only {
        println!(
            "world {}; body {}\nparse_id {}; ambiguity {}; nodes {}; holes {}",
            analysis.file.world_name,
            analysis.file.body_text,
            analysis.parse_id,
            parse_count(&analysis.parse_forest_json, "ambiguity_count"),
            parse_count(&analysis.parse_forest_json, "node_count"),
            analysis.file.explore.len()
        );
    } else {
        println!(
            "world {}; body: {}; explore: {}; protect: {}; answer: {}",
            analysis.file.world_name,
            analysis.file.body_text,
            analysis.file.explore.join(","),
            analysis.file.protect.join(","),
            analysis.file.answer
        );
    }
    if let Err(error) = write_if_requested(out, "parse-forest.json", &analysis.parse_forest_json) {
        eprintln!("error: {error}");
        return EXIT_USAGE;
    }
    EXIT_OK
}

/// Best-effort numeric field reader for CLI summaries over single-line JSON.
fn parse_count(json: &str, field: &str) -> String {
    let needle = format!("\"{field}\":");
    json.find(&needle).map_or_else(
        || "?".to_string(),
        |start| {
            let rest = &json[start + needle.len()..];
            let digits = rest
                .chars()
                .take_while(char::is_ascii_digit)
                .collect::<String>();
            if digits.is_empty() {
                "?".to_string()
            } else {
                digits
            }
        },
    )
}

/// `signature <file> [--out <dir>]`: signature + fixity + type variables.
pub fn signature_cmd(path: &Path, out: Option<&PathBuf>) -> u8 {
    let analysis = match analyze(path) {
        Ok(analysis) => analysis,
        Err(error) => {
            eprintln!("error: {error}");
            return EXIT_REFUSED;
        }
    };
    let mut arities = String::new();
    for (symbol, arity) in analysis.inference.signature.iter() {
        let _ = writeln!(arities, "  {}: {arity}", symbol.0);
    }
    println!(
        "signature_id {}; world {}\nsymbols:\n{}variables: {:?}",
        analysis.signature_id, analysis.file.world_name, arities, analysis.inference.variables
    );
    if let Err(error) = write_if_requested(out, "signature.json", &analysis.signature_json) {
        eprintln!("error: {error}");
        return EXIT_USAGE;
    }
    EXIT_OK
}

/// Evaluates `analysis.term` in `world` with the same fixture valuations
/// the parametric lane (`compile --parametric`) generates, so a genesis
/// receipt can never contradict it.
///
/// Returns `(answer, valuation_label)`: when the term evaluates (its
/// free variables are covered by the fixture), the answer is the
/// evaluated value and the label names the fixture; otherwise the answer
/// is the term's structural canonical form and the label is
/// `structural`. Answers are never fabricated constants: the old
/// hardcoded `6`/`false` invented meaning out of thin air and stamped it
/// `tested`.
fn evaluated_answer(analysis: &Analysis, world: &WorldIr) -> (String, &'static str) {
    let canonical = analysis.term.canonical();
    let free_env: Environment<Term> = [
        (
            VariableId("a".to_string()),
            Term::Variable(VariableId("a".to_string())),
        ),
        (
            VariableId("b".to_string()),
            Term::Variable(VariableId("b".to_string())),
        ),
    ]
    .into();
    let boolean_env: Environment<bool> = [
        (VariableId("a".to_string()), true),
        (VariableId("b".to_string()), false),
    ]
    .into();
    let modular_env: Environment<i64> = [
        (VariableId("a".to_string()), 4),
        (VariableId("b".to_string()), 7),
    ]
    .into();
    match world.name.as_str() {
        "free_symbolic" => match evaluate(&analysis.term, &FreeTermWorld, &free_env) {
            Ok(value) => (value.canonical(), "fixture_free"),
            Err(_) => (canonical, "structural"),
        },
        "Boolean_algebra" => match evaluate(&analysis.term, &BooleanAlienWorld, &boolean_env) {
            Ok(value) => (value.to_string(), "fixture_boolean"),
            Err(_) => (canonical, "structural"),
        },
        "modular_numeric" => match evaluate(&analysis.term, &ModularAlienWorld, &modular_env) {
            Ok(value) => (value.to_string(), "fixture_modular"),
            Err(_) => (canonical, "structural"),
        },
        _ => (canonical, "structural"),
    }
}

/// Honest portfolio for the built-in seed worlds: every candidate is a
/// real evaluation (or the structural term) with its valuation disclosed
/// in the provenance, and authority is Structural — no checker ran, so
/// nothing is stamped `tested` from `checker_receipts: []`.
fn portfolio(analysis: &Analysis, worlds: &[WorldIr]) -> InterpretationPortfolio {
    let candidates = worlds
        .iter()
        .map(|world| {
            let label = world.name.as_str();
            let (answer, valuation) = evaluated_answer(analysis, world);
            let (cost, complexity, utility) = match label {
                "free_symbolic" => (1.0, 2.0, 2.0),
                "Boolean_algebra" => (3.0, 1.0, 4.0),
                _ => (4.0, 2.0, 5.0),
            };
            InterpretationCandidate {
                world_id: world.identity(),
                name: label.into(),
                answer,
                authority: Authority::Structural,
                score: ScoreVector {
                    cost,
                    complexity,
                    // No checker ran: evidence stays zero and the
                    // receipt's checker_receipts list stays empty.
                    evidence: 0.0,
                    utility,
                },
                provenance: format!("builtin-seed;valuation={valuation}"),
            }
        })
        .collect();
    InterpretationPortfolio::new(candidates)
}

/// `genesis <file> --out <dir>`: full analysis artifact set.
pub fn genesis_cmd(path: &Path, out: &PathBuf) -> u8 {
    let analysis = match analyze(path) {
        Ok(analysis) => analysis,
        Err(error) => {
            eprintln!("error: {error}");
            return EXIT_REFUSED;
        }
    };
    if let Err(error) = fs::create_dir_all(out) {
        eprintln!("error: cannot create {}: {error}", out.display());
        return EXIT_USAGE;
    }
    let worlds = builtin_worlds(&analysis.inference.signature);
    let mut raw_portfolio = portfolio(&analysis, &worlds);
    // Honor `keep: pareto N`: the portfolio holds at most the N
    // policy-best candidates. A smaller budget must change the artifact
    // instead of silently presenting the full tie set as one winner.
    if let Some(budget) = analysis.file.keep_pareto {
        if budget == 0 {
            eprintln!("error: E-GEN-093: `keep: pareto 0` keeps no candidates");
            return EXIT_REFUSED;
        }
        let kept = raw_portfolio
            .candidates()
            .iter()
            .take(usize::try_from(budget).unwrap_or(usize::MAX))
            .cloned()
            .collect::<Vec<_>>();
        raw_portfolio = InterpretationPortfolio::new(kept);
    }
    let portfolio = raw_portfolio;
    // An explicit `answer: return interpretation_portfolio` asks for the
    // whole portfolio as the answer; without it, the single best
    // candidate is the answer. Either way authority stays Structural:
    // `checker_receipts` is empty and no `tested` stamp is invented.
    let portfolio_request = analysis.file.answer.contains("interpretation_portfolio");

    let free_term = {
        let mut object = emath_artifact::JsonWriter::object();
        object.string("schema", "emath.free-term.v1");
        object.int("term_id", analysis.term_id);
        object.string("canonical", &analysis.term.canonical());
        object.finish()
    };
    let meaning_problem = {
        let mut object = emath_artifact::JsonWriter::object();
        object.string("schema", "emath.meaning-problem.v1");
        object.string("world_name", &analysis.file.world_name);
        object.int("signature_id", analysis.signature_id);
        object.int("term_id", analysis.term_id);
        object.strings("constraints", &analysis.file.protect);
        object.strings("examples", &[]);
        object.finish()
    };
    let portfolio_json = {
        let mut object = emath_artifact::JsonWriter::object();
        object.string("schema", "emath.interpretation-portfolio.v1");
        object.int(
            "portfolio_id",
            fnv1a64(
                portfolio
                    .candidates()
                    .iter()
                    .map(|c| format!("{}:{}", c.world_id.0, c.name))
                    .collect::<Vec<_>>()
                    .join("|")
                    .as_bytes(),
            ),
        );
        let mut entries = String::new();
        for (index, candidate) in portfolio.candidates().iter().enumerate() {
            if index > 0 {
                entries.push(',');
            }
            let _ = write!(
                entries,
                "{{\"world_id\":\"{:016x}\",\"name\":\"{}\",\"answer\":\"{}\",\"authority\":\"{}\",\"score\":{{\"cost\":{},\"complexity\":{},\"evidence\":{},\"utility\":{}}},\"provenance\":\"{}\"}}",
                candidate.world_id.0,
                candidate.name,
                candidate.answer,
                authority_str(candidate.authority),
                candidate.score.cost,
                candidate.score.complexity,
                candidate.score.evidence,
                candidate.score.utility,
                candidate.provenance
            );
        }
        object.object_field("candidates", &format!("[{entries}]"));
        object.finish()
    };

    let mut admission = String::new();
    let mut completed = Vec::<String>::new();
    for (seq, label) in analysis.file.explore.iter().enumerate() {
        let seq = u64::try_from(seq).unwrap_or(u64::MAX);
        if let Some(world) = worlds.iter().find(|world| world.name == *label) {
            let world_id = world.identity().0;
            completed.push(label.clone());
            let target = out
                .join("world-candidates")
                .join(format!("{world_id:016x}.json"));
            let Some(parent) = target.parent() else {
                eprintln!("error: world-candidates has no parent");
                return EXIT_USAGE;
            };
            if fs::create_dir_all(parent).is_err() {
                eprintln!("error: cannot create world-candidates");
                return EXIT_USAGE;
            }
            let receipt = format!("{:016x}", fnv1a64(world.canonical().as_bytes()));
            let body = {
                let mut object = emath_artifact::JsonWriter::object();
                object.string("schema", "emath.world-candidate.v1");
                let id_hex = format!("{world_id:016x}");
                object.string("world_id", &id_hex);
                object.string("name", label);
                object.string("provider_id", "builtin-seed");
                object.strings("claimed_obligations", &analysis.file.protect);
                object.string("proposal_receipt", &receipt);
                object.finish()
            };
            if fs::write(&target, &body).is_err() {
                eprintln!("error: cannot write {}", target.display());
                return EXIT_USAGE;
            }
            admission.push_str(&jsonl(
                seq,
                "emath.world-admission.v1",
                "admitted",
                "ok",
                label,
                Some(world_id),
            ));
        } else {
            let code = if label == "matrix" || label == "graph" {
                "E-GEN-090"
            } else {
                "E-GEN-091"
            };
            admission.push_str(&jsonl(
                seq,
                "emath.world-admission.v1",
                "deferred",
                code,
                label,
                None,
            ));
        }
        admission.push('\n');
    }

    let kept = portfolio.candidates();
    let selected = kept.first();
    let result_string = if portfolio_request {
        kept.iter()
            .map(|candidate| format!("{}:{}", candidate.name, candidate.answer))
            .collect::<Vec<_>>()
            .join(";")
    } else {
        selected.map_or_else(String::new, |candidate| candidate.answer.clone())
    };
    let answer_anchor = if portfolio_request {
        let portfolio_id = fnv1a64(
            kept.iter()
                .map(|candidate| format!("{}", candidate.world_id.0))
                .collect::<Vec<_>>()
                .join("|")
                .as_bytes(),
        );
        format!("{portfolio_id:016x}")
    } else {
        selected
            .map(|candidate| format!("{:016x}", candidate.world_id.0))
            .unwrap_or_default()
    };
    let answer_id = format!(
        "{:016x}",
        fnv1a64(format!("{}-{answer_anchor}", analysis.parse_id).as_bytes())
    );
    let valuation = if portfolio_request {
        kept.iter()
            .map(|candidate| format!("{}={}", candidate.name, valuation_label(candidate)))
            .collect::<Vec<_>>()
            .join(";")
    } else {
        selected.map_or_else(
            || "structural".to_string(),
            |candidate| valuation_label(candidate).to_string(),
        )
    };
    let answer_authority = kept
        .iter()
        .map(|candidate| candidate.authority)
        .max()
        .unwrap_or(Authority::Structural);
    let answer_receipt = {
        let mut object = emath_artifact::JsonWriter::object();
        object.string("schema", "emath.answer-receipt.v1");
        object.string("answer_id", &answer_id);
        object.int("source_hash", analysis.source_hash);
        object.int("parse_id", analysis.parse_id);
        object.int("signature_id", analysis.signature_id);
        object.string("world_id", &answer_anchor);
        object.string("valuation", &valuation);
        object.strings("provider_locks", &completed);
        object.strings("checker_receipts", &[]);
        object.int("artifact_hash", 0);
        object.string("target", &path_to_string(path));
        object.string("result", &result_string);
        object.int("trace_hash", fnv1a64(admission.as_bytes()));
        object.string("authority", authority_str(answer_authority));
        object.finish()
    };

    let files = [
        ("parse-forest.json", &analysis.parse_forest_json),
        ("signature.json", &analysis.signature_json),
        ("free-term.json", &free_term),
        ("meaning-problem.json", &meaning_problem),
        ("interpretation-portfolio.json", &portfolio_json),
        ("world-admission.jsonl", &admission),
        ("answer-receipt.json", &answer_receipt),
    ];
    for (name, body) in files {
        let target = out.join(name);
        if let Err(error) = fs::write(&target, body) {
            eprintln!("error: cannot write {}: {error}", target.display());
            return EXIT_USAGE;
        }
    }
    for world in &worlds {
        println!("world {} {:016x}", world.name, world.identity().0);
    }
    println!(
        "genesis {}: parse {} signature {} term {:016x} portfolio {} kept {} answer {}",
        path_to_string(path),
        analysis.parse_id,
        analysis.signature_id,
        analysis.term_id,
        fnv1a64(portfolio_json.as_bytes()),
        kept.len(),
        if portfolio_request {
            "interpretation_portfolio".to_string()
        } else {
            kept.first().map_or_else(String::new, |c| c.name.clone())
        }
    );
    EXIT_OK
}

/// Valuation label disclosed on a candidate's provenance
/// (`builtin-seed;valuation=<label>`), or `structural` when only the
/// canonical term backs the answer.
fn valuation_label(candidate: &InterpretationCandidate) -> &str {
    candidate
        .provenance
        .rsplit_once('=')
        .map_or("structural", |(_, label)| label)
}

fn authority_str(authority: Authority) -> &'static str {
    match authority {
        Authority::Structural => "structural",
        Authority::Tested => "tested",
        Authority::Certified => "certified",
        Authority::Proved => "proved",
    }
}

fn path_to_string(path: &Path) -> String {
    path.display().to_string()
}

/// `compile --parametric <file> --out <dir>`: emit the generated crate.
pub fn compile_cmd(path: &Path, out: &Path, worlds: &[String]) -> u8 {
    let analysis = match analyze(path) {
        Ok(analysis) => analysis,
        Err(error) => {
            eprintln!("error: {error}");
            return EXIT_REFUSED;
        }
    };
    let world_labels = if worlds.is_empty() {
        ADMITTED_WORLDS
            .iter()
            .map(|label| (*label).to_string())
            .collect::<Vec<_>>()
    } else {
        for label in worlds {
            if !ADMITTED_WORLDS.contains(&label.as_str()) {
                eprintln!("error: E-GEN-092: unknown world `{label}`");
                return EXIT_REFUSED;
            }
        }
        worlds.to_vec()
    };
    // Generator labels are lowercase stable IDs; surface labels may be
    // authored as `Boolean_algebra` in explore clauses.
    let spec_labels = world_labels
        .iter()
        .map(|label| label.to_ascii_lowercase())
        .collect::<Vec<_>>();
    // SURF-0008: codegen emits a fixed per-label interpretation, so the
    // analyzed WorldIr operator semantics are handed to the generator;
    // it refuses (E-GEN-094) any map it cannot honor instead of
    // silently dropping the unused WorldIr.
    let worlds = builtin_worlds(&analysis.inference.signature);
    let specs = world_labels
        .iter()
        .map(|label| {
            let lower = label.to_ascii_lowercase();
            let operators = worlds
                .iter()
                .find(|world| world.name == *label)
                .map(|world| {
                    world
                        .operators
                        .iter()
                        .filter_map(|operator| match &operator.semantics {
                            emath_world_ir::OperatorSemantics::DeclaredExpression(meaning) => {
                                Some((operator.symbol.0.clone(), meaning.clone()))
                            }
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            emath_world_codegen_rust::WorldSpec {
                label: lower,
                operators,
            }
        })
        .collect::<Vec<_>>();
    let generated = match emath_world_codegen_rust::generate(
        &analysis.term,
        &analysis.inference.signature,
        &specs,
    ) {
        Ok(generated) => generated,
        Err(refusal) => {
            eprintln!("error: {}: {}", refusal.code, refusal.message);
            return EXIT_REFUSED;
        }
    };
    if let Err(error) = generated.write_to(out) {
        eprintln!("error: cannot write generated crate: {error}");
        return EXIT_USAGE;
    }
    let manifest = {
        let mut object = emath_artifact::JsonWriter::object();
        object.string("schema", "emath.generated-crate-manifest.v1");
        object.string("crate_name", &generated.crate_name);
        object.string("source", &path_to_string(path));
        object.int("source_hash", analysis.source_hash);
        object.int("parse_id", analysis.parse_id);
        object.int("signature_id", analysis.signature_id);
        object.int("term_id", analysis.term_id);
        object.strings("worlds", &spec_labels);
        let mut files_str = String::from("[");
        for (index, rel) in generated.files.keys().enumerate() {
            if index > 0 {
                files_str.push(',');
            }
            let _ = write!(files_str, "\"{rel}\"");
        }
        files_str.push(']');
        object.object_field("files", &files_str);
        object.finish()
    };
    let source_map = emath_artifact::write_generated_crate_source_map(
        &path_to_string(path),
        &generated.files.keys().cloned().collect::<Vec<_>>(),
    );
    for (name, body) in [
        ("manifest.json", &manifest),
        ("source-map.json", &source_map),
    ] {
        let target = out.join(name);
        if let Err(error) = fs::write(&target, body) {
            eprintln!("error: cannot write {}: {error}", target.display());
            return EXIT_USAGE;
        }
    }
    println!(
        "generated crate {} → {} ({} files)",
        generated.crate_name,
        out.display(),
        generated.files.len()
    );
    EXIT_OK
}

/// `world show <id> [--dir <dir>]`.
pub fn world_show_cmd(id: &str, dir: &Path) -> u8 {
    let target = dir.join("world-candidates").join(format!("{id}.json"));
    match fs::read_to_string(&target) {
        Ok(body) => {
            print!("{body}");
            EXIT_OK
        }
        Err(error) => {
            eprintln!("error: {error}");
            EXIT_USAGE
        }
    }
}

/// `portfolio show <id> [--dir <dir>]`.
pub fn portfolio_show_cmd(id: &str, dir: &Path) -> u8 {
    let candidates = [
        dir.join(format!("interpretation-portfolio-{id}.json")),
        dir.join("interpretation-portfolio.json"),
    ];
    for path in candidates {
        if let Ok(body) = fs::read_to_string(&path) {
            print!("{body}");
            return EXIT_OK;
        }
    }
    eprintln!("error: no portfolio artifact under {}", dir.display());
    EXIT_USAGE
}
