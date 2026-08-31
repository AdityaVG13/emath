//! Semantic Genesis CLI: `parse`, `signature`, `genesis`, `compile
//! --parametric`, `world show`, `portfolio show`.
//!
//! Pipeline: source bytes → glyphs → parse forest → signature inference →
//! Term IR → free world → world candidates → interpretation portfolio →
//! answer receipt → parametric Rust artifact. Emitted JSON is deterministic
//! and std-only.

use super::{CliExit, CompileRequest, EXIT_OK, EXIT_REFUSED, EXIT_USAGE};
use emath_core::limits::Limits;
use emath_genesis::{
    forest, free_symbolic_world, run as vm_run, BooleanAlienWorld, Environment, FreeTermWorld,
    ModularAlienWorld, OnePointWorld, SeededCsaWorld, VmBudget, VmOutcome, CSA_MEANING_CLAIM,
    CSA_SCHEMA, CSA_SCHEMA_VERSION, VM_SCHEMA, VM_SCHEMA_VERSION,
};
use crate::portfolio::{
    apply_portfolio_cap, evaluate, Authority, CollapsePolicy, InterpretationCandidate,
    InterpretationPolicy, InterpretationPortfolio, MetricAxis, MetricPolarity, PortfolioError,
    ScoreVector, PROVENANCE_USER_LOCKED,
};
use emath_syntax::genesis as genesis_syntax;
use emath_term::{Signature, Term, VariableId, TERM_IR_VERSION};
use emath_world_ir::{
    fnv1a64, Fixity, MeaningOrigin, OperatorDef, OperatorSemantics, SymbolDef, WorldIr,
};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Component, Path, PathBuf};

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
    /// Raw UTF-8 source, byte-exact, for the sealed source artifact.
    pub source: String,
}

/// Reads, parses, and structurally analyzes a genesis source file.
pub fn analyze(path: &Path) -> Result<Analysis, String> {
    let source = fs::read_to_string(path).map_err(|error| {
        format!(
            "E-PKG-080: cannot read source file ({}: {error})",
            path.display()
        )
    })?;
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
        source,
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
    let mut object = emath_artifact::JsonWriter::object();
    object.int("seq", seq);
    object.string("schema", schema);
    object.string("status", status);
    object.string("code", code);
    object.string("label", label);
    if let Some(value) = world_id {
        object.string("world_id", &format!("{value:016x}"));
    }
    object.finish().chars().filter(|ch| *ch != '\n').collect()
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
                "one_point" => "One".into(),
                "csa_seeded" => "U64".into(),
                _ => "FreeTerm".into(),
            },
        }],
        symbols,
        operators,
        constructors: vec!["Element -> Constant/Apply".into()],
        laws: vec!["total".into(), "deterministic".into()],
        effects: vec![],
        holes: vec![],
        capabilities: vec!["pure".into()],
    }
}

/// Admitted built-in world labels (G4 gate: at least five world classes
/// with deterministic identities in the portfolio).
const ADMITTED_WORLDS: [&str; 5] = [
    "free_symbolic",
    "Boolean_algebra",
    "modular_numeric",
    "one_point",
    "csa_seeded",
];

/// Worlds with a Rust codegen lowering (`compile --parametric`). The
/// one-point and seeded-CSA totality witnesses are portfolio candidates
/// only: the generator has no lowering for them and must refuse rather
/// than emit an unhonored map.
const COMPILED_WORLDS: [&str; 3] = ["free_symbolic", "Boolean_algebra", "modular_numeric"];

/// Builds the five admitted `WorldIr` candidates for `signature`.
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
    // The degenerate one-point algebra: every symbol means the single
    // carrier point (ADR-003 totality witness, never intended meaning).
    worlds.push(declared_world(
        "one_point",
        signature,
        &[("ζ", "•"), ("⋈", "•"), ("⧖", "•"), ("⊛", "•")],
    ));
    // The canonical seeded algebra: total, deterministic, seed-keyed
    // FNV-1a mixing over u64 (emath.csa v1, baseline seed).
    worlds.push(declared_world(
        "csa_seeded",
        signature,
        &[
            ("ζ", "fnv1a(seed, const:ζ)"),
            ("⋈", "fnv1a(seed, apply:⋈, args)"),
            ("⧖", "fnv1a(seed, apply:⧖, args)"),
            ("⊛", "fnv1a(seed, apply:⊛, args)"),
        ],
    ));
    debug_assert!(
        worlds
            .iter()
            .map(|world| world.name.as_str())
            .eq(ADMITTED_WORLDS),
        "builtin worlds must match the admitted-world roster"
    );
    worlds
}

/// `parse <file> [--out <dir>]`: glyphs + bounded parse forest.
pub fn parse_cmd(path: &Path, out: Option<&PathBuf>, forest_only: bool) -> CliExit {
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
pub fn signature_cmd(path: &Path, out: Option<&PathBuf>) -> CliExit {
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

/// Evaluate `analysis.term` in `world` with the parametric lane's fixtures.
/// Returns `(answer, valuation_label, vm_steps)`; suspensions/unbound vars
/// yield the structural canonical form — never a fabricated constant.
fn evaluated_answer(analysis: &Analysis, world: &WorldIr) -> (String, &'static str, u64) {
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
    let budget = VmBudget::seed_default();
    match world.name.as_str() {
        "free_symbolic" => match vm_run(&analysis.term, &FreeTermWorld, &free_env, &budget) {
            Ok(VmOutcome::Complete { value, steps, .. }) => {
                (value.canonical(), "fixture_free", steps)
            }
            Ok(VmOutcome::Suspended(_)) | Err(_) => (canonical, "structural", 0),
        },
        "Boolean_algebra" => {
            match vm_run(&analysis.term, &BooleanAlienWorld, &boolean_env, &budget) {
                Ok(VmOutcome::Complete { value, steps, .. }) => {
                    (value.to_string(), "fixture_boolean", steps)
                }
                Ok(VmOutcome::Suspended(_)) | Err(_) => (canonical, "structural", 0),
            }
        }
        "modular_numeric" => {
            match vm_run(&analysis.term, &ModularAlienWorld, &modular_env, &budget) {
                Ok(VmOutcome::Complete { value, steps, .. }) => {
                    (value.to_string(), "fixture_modular", steps)
                }
                Ok(VmOutcome::Suspended(_)) | Err(_) => (canonical, "structural", 0),
            }
        }
        "one_point" => {
            // The one-point carrier IS the unit type: a zero-sized-value
            // map is the honest environment for a one-point algebra.
            #[allow(clippy::zero_sized_map_values)]
            let env: Environment<()> = analysis
                .inference
                .variables
                .iter()
                .map(|variable| (variable.clone(), ()))
                .collect();
            match vm_run(&analysis.term, &OnePointWorld, &env, &budget) {
                Ok(VmOutcome::Complete { steps, .. }) => ("•".to_string(), "one_point", steps),
                Ok(VmOutcome::Suspended(_)) | Err(_) => (canonical, "structural", 0),
            }
        }
        "csa_seeded" => {
            let csa = SeededCsaWorld::baseline();
            let env: Environment<u64> = analysis
                .inference
                .variables
                .iter()
                .map(|variable| (variable.clone(), csa.variable_value(&variable.0)))
                .collect();
            match vm_run(&analysis.term, &csa, &env, &budget) {
                Ok(VmOutcome::Complete { value, steps, .. }) => {
                    (format!("{value:016x}"), "csa_baseline_seed", steps)
                }
                Ok(VmOutcome::Suspended(_)) | Err(_) => (canonical, "structural", 0),
            }
        }
        _ => (canonical, "structural", 0),
    }
}

/// Honest portfolio for the built-in seed worlds: every candidate is a
/// real evaluation (or the structural term) with disclosed valuation and
/// `Structural` authority (no checker ran, so never `tested`). Also
/// returns VM step counts per world for the receipt's metered cost.
fn portfolio(
    analysis: &Analysis,
    worlds: &[WorldIr],
) -> (InterpretationPortfolio, BTreeMap<String, u64>) {
    let mut vm_steps = BTreeMap::new();
    let candidates = worlds
        .iter()
        .map(|world| {
            let label = world.name.as_str();
            let (answer, valuation, steps) = evaluated_answer(analysis, world);
            vm_steps.insert(label.to_string(), steps);
            let (cost, complexity, utility) = match label {
                "free_symbolic" => (1.0, 2.0, 2.0),
                "Boolean_algebra" => (3.0, 1.0, 4.0),
                // Totality witnesses rank below the interpreting worlds:
                // they answer everything but claim no intended meaning.
                "one_point" => (0.5, 0.5, 1.0),
                "csa_seeded" => (2.0, 3.0, 1.5),
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
    (InterpretationPortfolio::new(candidates), vm_steps)
}

/// `genesis <file> --out <dir>`: full analysis artifact set.
pub fn genesis_cmd(path: &Path, out: &PathBuf) -> CliExit {
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
    let all_worlds = builtin_worlds(&analysis.inference.signature);
    let selection = match crate::meaning_cmd::resolve_locked_worlds(path, &analysis, all_worlds) {
        Ok(selection) => selection,
        Err(error) => {
            eprintln!("{error}");
            return EXIT_REFUSED;
        }
    };
    let worlds = selection.worlds;
    let meaning_lock = selection.lock;
    let cap = selection.cap;
    let (mut raw_portfolio, vm_steps) = portfolio(&analysis, &worlds);
    // Honor `keep: pareto N` / lock `portfolio_cap` / default pin-of-5
    // only when no lock committed a single world. A lock commits before
    // ranking; candidate generation above already ran on the locked set.
    if meaning_lock.is_none() {
        if cap == 0 {
            eprintln!("error: E-GEN-093: `keep: pareto 0` keeps no candidates");
            return EXIT_REFUSED;
        }
        let kept = apply_portfolio_cap(raw_portfolio.candidates(), cap);
        raw_portfolio = InterpretationPortfolio::new(kept);
    }
    let portfolio = raw_portfolio;
    // Explicit policy, never `kept.first()` as a hidden winner:
    // `answer: return interpretation_portfolio` keeps the bag; a lock
    // commits one world; otherwise `single-best` requires a unique bag
    // member (`E-GEN-095` if several remain). Authority stays Structural.
    let portfolio_request = analysis.file.answer.contains("interpretation_portfolio");
    let policy = answer_policy(portfolio_request, meaning_lock.as_ref());
    let g7_receipt = match evaluate(
        portfolio
            .candidates()
            .iter()
            .map(InterpretationCandidate::world_candidate)
            .collect(),
        vec![MetricAxis::new("cost", MetricPolarity::Minimize)],
        policy,
    ) {
        Ok(receipt) => receipt,
        Err(PortfolioError::AmbiguousSingleBest { .. }) => {
            eprintln!(
                "error: E-GEN-095: ambiguous portfolio: lock a world or request `answer: return interpretation_portfolio`"
            );
            return EXIT_REFUSED;
        }
        Err(error) => {
            eprintln!("error: E-GEN-095: {error}");
            return EXIT_REFUSED;
        }
    };

    let free_term = {
        let mut object = emath_artifact::JsonWriter::object();
        object.string("schema", "emath.free-term");
        object.int("schema_version", u64::from(TERM_IR_VERSION));
        object.int("term_id", analysis.term_id);
        object.string("canonical", &analysis.term.canonical());
        object.finish()
    };
    let meaning_problem = {
        let mut object = emath_artifact::JsonWriter::object();
        object.string("schema", "emath.meaning-problem");
        object.string("world_name", &analysis.file.world_name);
        object.int("signature_id", analysis.signature_id);
        object.int("term_id", analysis.term_id);
        object.strings("constraints", &analysis.file.protect);
        object.strings("examples", &[]);
        object.finish()
    };
    let portfolio_json = {
        let mut object = emath_artifact::JsonWriter::object();
        object.string("schema", "emath.interpretation-portfolio");
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
        let mut entries = Vec::new();
        for candidate in portfolio.candidates() {
            let mut row = emath_artifact::JsonWriter::object();
            row.string("world_id", &format!("{:016x}", candidate.world_id.0));
            row.string("name", &candidate.name);
            row.string("answer", &candidate.answer);
            row.string("authority", authority_str(candidate.authority));
            let mut score = emath_artifact::JsonWriter::object();
            score.field("cost", &candidate.score.cost.to_string());
            score.field("complexity", &candidate.score.complexity.to_string());
            score.field("evidence", &candidate.score.evidence.to_string());
            score.field("utility", &candidate.score.utility.to_string());
            row.object_field("score", score.finish().trim());
            row.string("provenance", &candidate.provenance);
            entries.push(row.finish().trim_end().to_string());
        }
        object.objects("candidates", &entries);
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
                object.string("schema", "emath.world-candidate");
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
                "emath.world-admission",
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
                "emath.world-admission",
                "deferred",
                code,
                label,
                None,
            ));
        }
        admission.push('\n');
    }

    let kept = portfolio.candidates();
    let selected = selected_from_receipt(kept, &g7_receipt.selected);
    let result_string = if portfolio_request {
        selected
            .iter()
            .map(|candidate| format!("{}:{}", candidate.name, candidate.answer))
            .collect::<Vec<_>>()
            .join(";")
    } else {
        selected
            .first()
            .map_or_else(String::new, |candidate| candidate.answer.clone())
    };
    let answer_anchor = if portfolio_request {
        let portfolio_id = fnv1a64(
            selected
                .iter()
                .map(|candidate| format!("{}", candidate.world_id.0))
                .collect::<Vec<_>>()
                .join("|")
                .as_bytes(),
        );
        format!("{portfolio_id:016x}")
    } else {
        selected
            .first()
            .map(|candidate| format!("{:016x}", candidate.world_id.0))
            .unwrap_or_default()
    };
    let answer_id = format!(
        "{:016x}",
        fnv1a64(format!("{}-{answer_anchor}", analysis.parse_id).as_bytes())
    );
    let valuation = if portfolio_request {
        selected
            .iter()
            .map(|candidate| format!("{}={}", candidate.name, valuation_label(candidate)))
            .collect::<Vec<_>>()
            .join(";")
    } else {
        selected.first().map_or_else(
            || "structural".to_string(),
            |candidate| valuation_label(candidate).to_string(),
        )
    };
    let answer_authority = kept
        .iter()
        .map(|candidate| candidate.authority)
        .max()
        .unwrap_or(Authority::Structural);
    // Metered VM cost of the evaluation the receipt certifies: the
    // selected candidate's step count, or the sum across the kept set
    // when the whole portfolio is the answer. Zero means the answer is
    // structural (no execution happened).
    let receipt_vm_steps = if portfolio_request {
        selected
            .iter()
            .map(|candidate| vm_steps.get(&candidate.name).copied().unwrap_or(0))
            .sum::<u64>()
    } else {
        selected
            .first()
            .and_then(|candidate| vm_steps.get(&candidate.name).copied())
            .unwrap_or(0)
    };
    // SG-09 code binding: hash the exact crate `compile --parametric`
    // renders for the default compiled worlds, so the receipt binds the
    // code lane the demo challenges against these VM answers. A codegen
    // refusal binds the explicit no-code value 0 (disclosed, never a
    // fabricated identity).
    let artifact_hash = {
        let labels = COMPILED_WORLDS
            .iter()
            .map(|label| (*label).to_string())
            .collect::<Vec<_>>();
        let specs = codegen_specs(&worlds, &labels);
        emath_world_ir::world_codegen_rust::generate(&analysis.term, &analysis.inference.signature, &specs)
            .map_or(0, |generated| {
                let rows = generated
                    .files
                    .iter()
                    .map(|(rel, body)| format!("{rel}:{:016x}", fnv1a64(body.as_bytes())))
                    .collect::<Vec<_>>();
                fnv1a64(rows.join(";").as_bytes())
            })
    };
    let portfolio_hash = fnv1a64(portfolio_json.as_bytes());
    let trace_hash = fnv1a64(admission.as_bytes());
    let authority_label = authority_str(answer_authority);
    // SG-09 receipt identity (No Naked Answer, ADR-004): FNV-1a64 over the
    // documented preimage below, binding source, parse, signature, term,
    // world, valuation, result, code, portfolio, trace, authority, and VM
    // cost. An independent verifier (xtask demo semantic-genesis)
    // re-extracts every bound field and recomputes this id; a tampered
    // field breaks the recomputation. Keep the preimage in sync with the
    // verifier in xtask/src/main.rs.
    let receipt_id = fnv1a64(
        format!(
            "receipt:v2:{answer_id}:{}:{}:{}:{}:{answer_anchor}:{valuation}:{result_string}:{artifact_hash:016x}:{portfolio_hash:016x}:{trace_hash:016x}:{authority_label}:{receipt_vm_steps}",
            analysis.source_hash, analysis.parse_id, analysis.signature_id, analysis.term_id
        )
        .as_bytes(),
    );
    let answer_receipt = {
        let mut object = emath_artifact::JsonWriter::object();
        object.string("schema", "emath.answer-receipt");
        object.int("schema_version", 2);
        object.string("receipt_id", &format!("{receipt_id:016x}"));
        object.string("answer_id", &answer_id);
        object.int("source_hash", analysis.source_hash);
        object.int("parse_id", analysis.parse_id);
        object.int("signature_id", analysis.signature_id);
        object.int("term_id", analysis.term_id);
        object.string("world_id", &answer_anchor);
        object.string("valuation", &valuation);
        object.strings("provider_locks", &completed);
        object.strings("checker_receipts", &[]);
        object.string("artifact_hash", &format!("{artifact_hash:016x}"));
        object.string("portfolio_hash", &format!("{portfolio_hash:016x}"));
        object.string("target", &path_to_string(path));
        object.string("result", &result_string);
        object.string("trace_hash", &format!("{trace_hash:016x}"));
        object.string("authority", authority_label);
        object.string("vm_schema", &format!("{VM_SCHEMA}.v{VM_SCHEMA_VERSION}"));
        object.int("vm_steps", receipt_vm_steps);
        if let Some(lock) = &meaning_lock {
            object.string("meaning_provenance", PROVENANCE_USER_LOCKED);
            object.string("lock_id", &format!("{:016x}", lock.lock_id));
            object.string(
                "lock_origin_receipt",
                &format!("{:016x}", lock.origin_receipt_id),
            );
            object.string("lock_method", &lock.method);
            object.string("lock_world", &format!("{:016x}", lock.fingerprint));
        }
        object.finish()
    };

    // CSA totality baseline (ADR-003): one reproducible concrete value for
    // the admitted term under the canonical seeded algebra, evaluated on
    // the semantic VM and labeled so it can never be read as intended
    // meaning. CSA is total, so a failure here is a defect worth refusing
    // on, never something to paper over with a fabricated value.
    let csa_baseline = {
        let csa = SeededCsaWorld::baseline();
        let csa_env: Environment<u64> = analysis
            .inference
            .variables
            .iter()
            .map(|variable| (variable.clone(), csa.variable_value(&variable.0)))
            .collect();
        let (value, steps) = match vm_run(&analysis.term, &csa, &csa_env, &VmBudget::seed_default())
        {
            Ok(VmOutcome::Complete { value, steps, .. }) => (value, steps),
            Ok(VmOutcome::Suspended(_)) | Err(_) => {
                eprintln!("error: E-GEN-094: CSA baseline evaluation failed on a total world");
                return EXIT_REFUSED;
            }
        };
        let mut object = emath_artifact::JsonWriter::object();
        object.string("schema", CSA_SCHEMA);
        object.int("schema_version", u64::from(CSA_SCHEMA_VERSION));
        object.int("seed", csa.seed);
        object.int("term_id", analysis.term_id);
        object.string("value", &format!("{value:016x}"));
        object.int("vm_steps", steps);
        object.string("meaning_claim", CSA_MEANING_CLAIM);
        object.finish()
    };

    // Sealed source artifact (G0/SG-03): the raw bytes' identity plus the
    // byte-exact glyph stream of the semantic body, so every downstream
    // id chains back to one sealed document instead of a loose file read.
    let source_artifact = {
        let glyphs = analysis
            .file
            .body_text
            .chars()
            .map(|glyph| glyph.to_string())
            .collect::<Vec<_>>();
        let mut object = emath_artifact::JsonWriter::object();
        object.string("schema", "emath.source-artifact");
        object.int("schema_version", 1);
        object.string("source", &path_to_string(path));
        object.int("source_hash", analysis.source_hash);
        object.int(
            "byte_len",
            u64::try_from(analysis.source.len()).unwrap_or(u64::MAX),
        );
        object.string("world_name", &analysis.file.world_name);
        object.string("body_text", &analysis.file.body_text);
        object.int(
            "glyph_count",
            u64::try_from(glyphs.len()).unwrap_or(u64::MAX),
        );
        object.strings("glyphs", &glyphs);
        object.int("parse_id", analysis.parse_id);
        object.finish()
    };

    let g7_receipt_body = g7_receipt.encode();
    let files = [
        ("source-artifact.json", &source_artifact),
        ("parse-forest.json", &analysis.parse_forest_json),
        ("signature.json", &analysis.signature_json),
        ("free-term.json", &free_term),
        ("meaning-problem.json", &meaning_problem),
        ("interpretation-portfolio.json", &portfolio_json),
        ("g7-portfolio-receipt.txt", &g7_receipt_body),
        ("world-admission.jsonl", &admission),
        ("answer-receipt.json", &answer_receipt),
        ("csa-baseline.json", &csa_baseline),
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
        "genesis {}: parse {} signature {} term {:016x} portfolio {} kept {} policy {} answer {}{}",
        path_to_string(path),
        analysis.parse_id,
        analysis.signature_id,
        analysis.term_id,
        portfolio_hash,
        kept.len(),
        g7_receipt.input.policy.canonical(),
        if portfolio_request {
            "interpretation_portfolio".to_string()
        } else {
            selected
                .first()
                .map_or_else(String::new, |candidate| candidate.name.clone())
        },
        if meaning_lock.is_some() {
            format!(" provenance {PROVENANCE_USER_LOCKED}")
        } else {
            String::new()
        }
    );
    EXIT_OK
}

/// Valuation label disclosed on a candidate's provenance
/// (`builtin-seed;valuation=<label>`), or `structural` when only the
/// canonical term backs the answer.
pub fn answer_policy(
    portfolio_request: bool,
    lock: Option<&crate::meaning_cmd::ResolvedLock>,
) -> InterpretationPolicy {
    if let Some(lock) = lock {
        InterpretationPolicy::UserLocked {
            lock_id: lock.lock_id,
            origin_receipt_id: lock.origin_receipt_id,
            method: lock.method.clone(),
        }
    } else if portfolio_request {
        InterpretationPolicy::Portfolio
    } else {
        InterpretationPolicy::SingleBest {
            collapse: CollapsePolicy::RequireUnique,
        }
    }
}

fn selected_from_receipt<'a>(
    kept: &'a [InterpretationCandidate],
    fingerprints: &[u64],
) -> Vec<&'a InterpretationCandidate> {
    fingerprints
        .iter()
        .filter_map(|fingerprint| {
            kept.iter()
                .find(|candidate| candidate.world_id.0 == *fingerprint)
        })
        .collect()
}

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

/// Codegen world specs for `labels`. SURF-0008: the generator refuses
/// (`E-GEN-094`) any declared meaning it cannot honor.
fn codegen_specs(
    worlds: &[WorldIr],
    labels: &[String],
) -> Vec<emath_world_ir::world_codegen_rust::WorldSpec> {
    labels
        .iter()
        .map(|label| {
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
            emath_world_ir::world_codegen_rust::WorldSpec {
                label: label.to_ascii_lowercase(),
                operators,
            }
        })
        .collect()
}

pub fn compile_cmd(request: CompileRequest) -> CliExit {
    let CompileRequest::Ready { path, out, worlds } = request;
    let analysis = match analyze(&path) {
        Ok(analysis) => analysis,
        Err(error) => {
            eprintln!("error: {error}");
            return EXIT_REFUSED;
        }
    };
    let all_worlds = builtin_worlds(&analysis.inference.signature);
    let selection = match crate::meaning_cmd::resolve_locked_worlds(&path, &analysis, all_worlds) {
        Ok(selection) => selection,
        Err(error) => {
            eprintln!("{error}");
            return EXIT_REFUSED;
        }
    };
    let world_labels = if let Some(lock) = &selection.lock {
        if !worlds.is_empty() {
            for label in &worlds {
                let Some(world) = selection.worlds.iter().find(|world| world.name == *label) else {
                    eprintln!("error: E-GEN-092: unknown world `{label}`");
                    return EXIT_REFUSED;
                };
                if world.identity().0 != lock.fingerprint {
                    eprintln!(
                        "error: E-LOCK-004: --world `{label}` disagrees with locked fingerprint {:016x}; re-open the portfolio with `emath meaning unset`",
                        lock.fingerprint
                    );
                    return EXIT_REFUSED;
                }
            }
        }
        let label = selection.worlds[0].name.clone();
        if !COMPILED_WORLDS.contains(&label.as_str()) {
            eprintln!(
                "error: E-LOCK-004: locked world `{label}` has no parametric lowering; re-open the portfolio with `emath meaning unset`"
            );
            return EXIT_REFUSED;
        }
        vec![label]
    } else if worlds.is_empty() {
        COMPILED_WORLDS
            .iter()
            .map(|label| (*label).to_string())
            .collect::<Vec<_>>()
    } else {
        for label in &worlds {
            if !COMPILED_WORLDS.contains(&label.as_str()) {
                eprintln!("error: E-GEN-092: unknown world `{label}`");
                return EXIT_REFUSED;
            }
        }
        worlds.clone()
    };
    // Generator labels are lowercase stable IDs; surface labels may be
    // authored as `Boolean_algebra` in explore clauses.
    let spec_labels = world_labels
        .iter()
        .map(|label| label.to_ascii_lowercase())
        .collect::<Vec<_>>();
    let worlds = builtin_worlds(&analysis.inference.signature);
    let specs = codegen_specs(&worlds, &world_labels);
    let generated = match emath_world_ir::world_codegen_rust::generate(
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
    if let Err(error) = generated.write_to(&out) {
        eprintln!("error: cannot write generated crate: {error}");
        return EXIT_USAGE;
    }
    let manifest = {
        let mut object = emath_artifact::JsonWriter::object();
        object.string("schema", "emath.generated-crate-manifest");
        object.int("schema_version", 1);
        object.int(
            "world_abi_version",
            u64::from(emath_world_ir::world_codegen_rust::WORLD_ABI_VERSION),
        );
        object.string("crate_name", &generated.crate_name);
        object.string("source", &path_to_string(&path));
        object.int("source_hash", analysis.source_hash);
        object.int("parse_id", analysis.parse_id);
        object.int("signature_id", analysis.signature_id);
        object.int("term_id", analysis.term_id);
        object.strings("worlds", &spec_labels);
        let files: Vec<String> = generated.files.keys().cloned().collect();
        object.strings("files", &files);
        object.finish()
    };
    let source_map = emath_artifact::write_generated_crate_source_map(
        &path_to_string(&path),
        &generated.files.keys().cloned().collect::<Vec<_>>(),
    );
    // Hole manifest (SG-05/G3): in the parametric lane every signature
    // symbol's meaning is an open parameter supplied by a `World`
    // implementation. One deterministic entry per symbol, sorted by id.
    let hole_manifest = {
        let mut object = emath_artifact::JsonWriter::object();
        object.string("schema", "emath.hole-manifest");
        object.int("schema_version", 1);
        object.int("term_id", analysis.term_id);
        object.int("signature_id", analysis.signature_id);
        let mut symbols: Vec<(String, usize)> = analysis
            .inference
            .signature
            .iter()
            .map(|(symbol, arity)| (symbol.0.clone(), *arity))
            .collect();
        symbols.sort();
        let mut entries = Vec::new();
        for (symbol, arity) in &symbols {
            let kind = if *arity == 0 {
                "constant-definition"
            } else {
                "operator-definition"
            };
            let hole_id = format!(
                "{:016x}",
                fnv1a64(format!("hole:{symbol}:{arity}").as_bytes())
            );
            let mut hole = emath_artifact::JsonWriter::object();
            hole.string("hole_id", &hole_id);
            hole.string("symbol", symbol);
            hole.int("arity", u64::try_from(*arity).unwrap_or(u64::MAX));
            hole.string("kind", kind);
            hole.string("state", "open");
            hole.string("constraint", "meaning supplied by a World implementation");
            entries.push(hole.finish().trim_end().to_string());
        }
        object.objects("holes", &entries);
        object.finish()
    };
    for (name, body) in [
        ("manifest.json", &manifest),
        ("source-map.json", &source_map),
        ("hole-manifest.json", &hole_manifest),
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

/// Single path component under `--dir`. Rejects `..`, absolute, and nested
/// ids so `world show` / `portfolio show` cannot read outside the artifact dir.
pub fn confined_artifact_id(id: &str) -> bool {
    if id.is_empty() || id.contains('\0') {
        return false;
    }
    let path = Path::new(id);
    let mut parts = path.components();
    matches!(parts.next(), Some(Component::Normal(name)) if name == std::ffi::OsStr::new(id))
        && parts.next().is_none()
}

/// `world show <id> [--dir <dir>]`.
pub fn world_show_cmd(id: &str, dir: &Path) -> CliExit {
    if !confined_artifact_id(id) {
        eprintln!("error: E-GEN-096: world id is not a single path component");
        return EXIT_USAGE;
    }
    let target = dir.join("world-candidates").join(format!("{id}.json"));
    match fs::read_to_string(&target) {
        Ok(body) => {
            if let Some(code) = refuse_truncated_json(&target, &body) {
                return code;
            }
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
pub fn portfolio_show_cmd(id: &str, dir: &Path) -> CliExit {
    if !confined_artifact_id(id) {
        eprintln!("error: E-GEN-096: portfolio id is not a single path component");
        return EXIT_USAGE;
    }
    let candidates = [
        dir.join(format!("interpretation-portfolio-{id}.json")),
        dir.join("interpretation-portfolio.json"),
    ];
    for path in candidates {
        if let Ok(body) = fs::read_to_string(&path) {
            if let Some(code) = refuse_truncated_json(&path, &body) {
                return code;
            }
            print!("{body}");
            let g7 = dir.join("g7-portfolio-receipt.txt");
            if let Ok(receipt) = fs::read_to_string(&g7) {
                println!();
                print!("{receipt}");
            }
            for id in json_world_ids(&body) {
                eprintln!("hint: emath meaning set FILE.emath --world {id}");
            }
            return EXIT_OK;
        }
    }
    eprintln!("error: no portfolio artifact under {}", dir.display());
    EXIT_USAGE
}

fn refuse_truncated_json(path: &Path, body: &str) -> Option<CliExit> {
    if emath_artifact::parse_json_document(body).is_err() {
        eprintln!("error: truncated or malformed JSON in {}", path.display());
        Some(EXIT_REFUSED)
    } else {
        None
    }
}

fn json_world_ids(body: &str) -> Vec<String> {
    let mut ids = Vec::new();
    let mut rest = body;
    while let Some(index) = rest.find("\"world_id\"") {
        rest = &rest[index + 10..];
        let Some(start) = rest.find('"') else {
            break;
        };
        rest = &rest[start + 1..];
        let Some(end) = rest.find('"') else {
            break;
        };
        ids.push(rest[..end].to_string());
        rest = &rest[end + 1..];
    }
    ids
}
