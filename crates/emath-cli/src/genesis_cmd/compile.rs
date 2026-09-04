//! The `emath compile` and world/portfolio show commands.

use super::*;

/// Codegen world specs for `labels`. SURF-0008: the generator refuses
/// (`E-GEN-094`) any declared meaning it cannot honor.
pub(super) fn codegen_specs(
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

pub(super) fn refuse_truncated_json(path: &Path, body: &str) -> Option<CliExit> {
    if emath_artifact::parse_json_document(body).is_err() {
        eprintln!("error: truncated or malformed JSON in {}", path.display());
        Some(EXIT_REFUSED)
    } else {
        None
    }
}

pub(super) fn json_world_ids(body: &str) -> Vec<String> {
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
