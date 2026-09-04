//! The `emath genesis` command: full pipeline.

use super::*;

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
        emath_world_ir::world_codegen_rust::generate(
            &analysis.term,
            &analysis.inference.signature,
            &specs,
        )
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

pub(super) fn selected_from_receipt<'a>(
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

pub(super) fn valuation_label(candidate: &InterpretationCandidate) -> &str {
    candidate
        .provenance
        .rsplit_once('=')
        .map_or("structural", |(_, label)| label)
}

pub(super) fn authority_str(authority: Authority) -> &'static str {
    match authority {
        Authority::Structural => "structural",
        Authority::Tested => "tested",
        Authority::Certified => "certified",
        Authority::Proved => "proved",
    }
}

pub(super) fn path_to_string(path: &Path) -> String {
    path.display().to_string()
}
