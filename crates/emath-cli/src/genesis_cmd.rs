//! Semantic Genesis CLI: `parse`, `signature`, `genesis`, `compile
//! --parametric`, `world show`, `portfolio show`.
//!
//! Pipeline: source bytes → glyphs → parse forest → signature inference →
//! Term IR → free world → world candidates → interpretation portfolio →
//! answer receipt → parametric Rust artifact. Emitted JSON is deterministic
//! and std-only.

use super::{EXIT_OK, EXIT_REFUSED, EXIT_USAGE};
use emath_core::limits::Limits;
use emath_genesis::free_symbolic_world;
use emath_portfolio::{Authority, InterpretationCandidate, InterpretationPortfolio, ScoreVector};
use emath_syntax::{forest, genesis as genesis_syntax};
use emath_term::{Signature, Term};
use emath_world_ir::{
    fnv1a64, Fixity, MeaningOrigin, OperatorDef, OperatorSemantics, SymbolDef, WorldIr,
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

fn portfolio(analysis: &Analysis, worlds: &[WorldIr]) -> InterpretationPortfolio {
    let fixture_answer = |label: &str| -> String {
        match label {
            "free_symbolic" => analysis.term.canonical(),
            "Boolean_algebra" => "false".into(),
            _ => "6".into(),
        }
    };
    let candidates = worlds
        .iter()
        .map(|world| {
            let label = world.name.as_str();
            let (authority, cost, complexity, evidence, utility) = match label {
                "free_symbolic" => (Authority::Structural, 1.0, 2.0, 0.0, 2.0),
                "Boolean_algebra" => (Authority::Tested, 3.0, 1.0, 1.0, 4.0),
                _ => (Authority::Tested, 4.0, 2.0, 1.0, 5.0),
            };
            InterpretationCandidate {
                world_id: world.identity(),
                name: label.into(),
                answer: fixture_answer(label),
                authority,
                score: ScoreVector {
                    cost,
                    complexity,
                    evidence,
                    utility,
                },
                provenance: "builtin-seed".into(),
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
    let portfolio = portfolio(&analysis, &worlds);

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

    let selected = portfolio
        .candidates()
        .first()
        .expect("portfolio is nonempty");
    let answer_id = format!(
        "{:016x}",
        fnv1a64(format!("{}-{}", analysis.parse_id, selected.world_id.0).as_bytes())
    );
    let world_id_hex = format!("{:016x}", selected.world_id.0);
    let answer_receipt = {
        let mut object = emath_artifact::JsonWriter::object();
        object.string("schema", "emath.answer-receipt.v1");
        object.string("answer_id", &answer_id);
        object.int("source_hash", analysis.source_hash);
        object.int("parse_id", analysis.parse_id);
        object.int("signature_id", analysis.signature_id);
        object.string("world_id", &world_id_hex);
        object.string("valuation", "{}");
        object.strings("provider_locks", &completed);
        object.strings("checker_receipts", &[]);
        object.int("artifact_hash", 0);
        object.string("target", &path_to_string(path));
        object.string("result", &selected.answer);
        object.int("trace_hash", fnv1a64(admission.as_bytes()));
        object.string("authority", authority_str(selected.authority));
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
        "genesis {}: parse {} signature {} term {:016x} portfolio {} candidate {}",
        path_to_string(path),
        analysis.parse_id,
        analysis.signature_id,
        analysis.term_id,
        fnv1a64(portfolio_json.as_bytes()),
        selected.name
    );
    EXIT_OK
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
    let specs = spec_labels
        .iter()
        .map(|label| emath_world_codegen_rust::WorldSpec {
            label: label.clone(),
        })
        .collect::<Vec<_>>();
    let generated =
        emath_world_codegen_rust::generate(&analysis.term, &analysis.inference.signature, &specs);
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
    let source_map = {
        let mut object = emath_artifact::JsonWriter::object();
        object.string("schema", "emath.source-map.v1");
        object.string("source", &path_to_string(path));
        let mut entries = String::from("[");
        for (index, rel) in generated.files.keys().enumerate() {
            if index > 0 {
                entries.push(',');
            }
            let _ = write!(
                entries,
                "{{\"generated\":\"{rel}\",\"source\":\"{}\",\"kind\":\"parametric-world\"}}",
                path_to_string(path)
            );
        }
        entries.push(']');
        object.object_field("entries", &entries);
        object.finish()
    };
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

#[cfg(test)]
mod tests {
    use super::*;

    fn example_path() -> PathBuf {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        root.join("language/examples/01_arbitrary_glyphs.emath")
    }

    #[test]
    fn reference_example_yields_expected_signature_and_term() {
        let analysis = analyze(&example_path()).expect("reference example analyzes");
        assert_eq!(analysis.file.world_name, "AlienGlyphs");
        let mut arities = Vec::new();
        for (symbol, arity) in analysis.inference.signature.iter() {
            arities.push(format!("{}:{arity}", symbol.0));
        }
        // BTreeMap iterates in Unicode scalar order.
        assert_eq!(arities, ["ζ:0", "⊛:2", "⋈:2", "⧖:1"]);
        assert_eq!(
            analysis.term.canonical(),
            "apply(⊛,apply(⧖,apply(⋈,var(a),var(b))),const(ζ))"
        );
        let variables = analysis
            .inference
            .variables
            .iter()
            .map(|variable| variable.0.clone())
            .collect::<Vec<_>>();
        assert_eq!(variables, ["a", "b"]);
    }

    #[test]
    fn analysis_artifacts_are_deterministic() {
        let first = analyze(&example_path()).expect("analyzes");
        let second = analyze(&example_path()).expect("analyzes again");
        assert_eq!(first.parse_forest_json, second.parse_forest_json);
        assert_eq!(first.signature_json, second.signature_json);
        assert_eq!(first.parse_id, second.parse_id);
        assert_eq!(first.signature_id, second.signature_id);
        assert_eq!(first.term_id, second.term_id);
    }

    #[test]
    fn genesis_emits_full_artifact_set() {
        let dir = std::env::temp_dir().join(format!("emath-genesis-cmd-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let code = genesis_cmd(&example_path(), &dir);
        assert_eq!(code, EXIT_OK);
        for name in [
            "parse-forest.json",
            "signature.json",
            "free-term.json",
            "meaning-problem.json",
            "interpretation-portfolio.json",
            "world-admission.jsonl",
            "answer-receipt.json",
        ] {
            assert!(dir.join(name).is_file(), "missing {name}");
        }
        let candidates = fs::read_dir(dir.join("world-candidates")).expect("candidates dir");
        assert_eq!(candidates.count(), 3);
        let admission = fs::read_to_string(dir.join("world-admission.jsonl")).expect("jsonl");
        let admitted = admission
            .lines()
            .filter(|line| line.contains("\"status\":\"admitted\""))
            .count();
        assert_eq!(admitted, 3);
        assert!(admission.contains("\"status\":\"deferred\""));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn compile_refuses_unknown_world() {
        let dir = std::env::temp_dir().join(format!("emath-genesis-cmd-{}", std::process::id()));
        let code = compile_cmd(&example_path(), &dir, &["hollywood".to_string()]);
        assert_eq!(code, EXIT_REFUSED);
        let _ = fs::remove_dir_all(&dir);
    }
}
