//! Forest analysis shared by all genesis commands.

use super::*;

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

pub(super) fn write_if_requested(
    out: Option<&PathBuf>,
    name: &str,
    body: &str,
) -> Result<(), String> {
    if let Some(dir) = out {
        fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        let target = dir.join(name);
        fs::write(&target, body).map_err(|e| format!("cannot write {}: {e}", target.display()))?;
    }
    Ok(())
}

/// Emits single-line JSON for JSONL records (`world-admission.jsonl`).
pub(super) fn jsonl(
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
