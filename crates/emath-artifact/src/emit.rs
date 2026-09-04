//! Canonical JSON serialization of artifacts and records.

use super::*;

pub(super) fn target_json(target: &TargetProfile) -> String {
    let triple = match &target.triple {
        Some(value) => quote(value),
        None => "null".to_string(),
    };
    let mut out = JsonWriter::object();
    out.string("family", &target.family);
    out.strings("features", &target.features);
    out.field("triple", &triple);
    out.finish()
}

/// Serialize a manifest per `emath.artifact`. Deterministic: files are
/// iterated in `BTreeMap` order.
pub fn write_artifact_manifest(manifest: &ArtifactManifest) -> String {
    let providers = manifest
        .providers
        .iter()
        .map(|p| {
            let mut out = JsonWriter::object();
            out.string("id", &p.id);
            out.string("version", &p.version);
            out.field("implementation", &content_id_or_empty(&p.implementation));
            out.finish()
        })
        .collect::<Vec<_>>()
        .join(",\n    ");
    let providers = if providers.is_empty() {
        "[]".to_string()
    } else {
        format!("[\n    {providers}\n  ]")
    };
    let files = manifest
        .files
        .iter()
        .map(|(path, id)| format!("    {}: {}", quote(path), quote(&id.0)))
        .collect::<Vec<_>>()
        .join(",\n");
    let mut out = JsonWriter::object();
    out.string("schema", "emath.artifact");
    out.field("artifact_id", &content_id_or_empty(&manifest.artifact_id));
    out.string("class", manifest.class.as_str());
    out.field(
        "source_package",
        &content_id_or_empty(&manifest.source_package),
    );
    out.field("compiler", &content_id_or_empty(&manifest.compiler));
    out.object_field("target", &target_json(&manifest.target));
    out.string("numeric_profile", &manifest.numeric_profile);
    out.object_field("providers", &providers);
    out.string("evidence_level", manifest.evidence_level.as_str());
    out.strings("public_exports", &manifest.public_exports);
    out.strings("assumptions", &manifest.assumptions);
    out.field("files", &format!("{{\n{files}\n  }}"));
    out.field("source_map", &content_id_or_empty(&manifest.source_map));
    out.field(
        "resolution_plan",
        &content_id_or_empty(&manifest.resolution_plan),
    );
    out.field(
        "evidence_bundle",
        &content_id_or_empty(&manifest.evidence_bundle),
    );
    out.finish()
}

/// Serialize a source map per `emath.source-map`.
pub fn write_source_map(source_map: &SourceMap) -> String {
    let mut object = JsonWriter::object();
    object.field("schema", &quote("emath.source-map"));
    object.field(
        "source_package",
        &content_id_or_empty(&source_map.source_package),
    );
    if source_map.entries.is_empty() {
        object.field("entries", "[]");
        return object.finish();
    }
    let entries = source_map
        .entries
        .iter()
        .map(|entry| {
            let mut out = JsonWriter::object();
            out.int("file", u64::from(entry.file));
            out.string("source_file", &entry.source_file);
            out.int("source_start", entry.source_start);
            out.int("source_end", entry.source_end);
            out.string("semantic_node", &entry.semantic_node);
            match &entry.plan_node {
                Some(node) => out.string("plan_node", node),
                None => out.string("plan_node", ""),
            };
            out.string("generated_file", &entry.generated_file);
            out.int("generated_start", entry.generated_start);
            out.int("generated_end", entry.generated_end);
            match &entry.generated_symbol {
                Some(symbol) => out.string("generated_symbol", symbol),
                None => out.string("generated_symbol", ""),
            };
            out.finish()
        })
        .collect::<Vec<_>>()
        .join(",\n    ");
    object.field("entries", &format!("[\n    {entries}\n  ]"));
    object.finish()
}

/// Write a world-codegen provenance source map
/// (`emath.generated-crate-source-map`); entries carry
/// `(generated, source, kind)` labels, never the durable artifact source
/// map shape. `files` must be in deterministic emission order.
pub fn write_generated_crate_source_map(source: &str, files: &[String]) -> String {
    let mut object = JsonWriter::object();
    // World-codegen provenance, not the durable artifact source map:
    // these entries carry (generated, source, kind) labels, not the
    // byte-range + source_package shape of `emath.source-map`.
    object.string("schema", GENERATED_CRATE_SOURCE_MAP_SCHEMA);
    object.string("source", source);
    let entries: Vec<String> = files
        .iter()
        .map(|rel| {
            let mut entry = JsonWriter::object();
            entry.string("generated", rel);
            entry.string("source", source);
            entry.string("kind", "parametric-world");
            entry.finish().trim_end().to_string()
        })
        .collect();
    object.objects("entries", &entries);
    object.finish()
}

/// One world-codegen provenance entry (`{generated, source, kind}`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeneratedCrateSourceMapEntry {
    /// Relative path inside the generated crate.
    pub generated: String,
    /// Source document the entry was derived from.
    pub source: String,
    /// Codegen kind (currently always `parametric-world`).
    pub kind: String,
}

/// Parsed world-codegen provenance source map
/// (`emath.generated-crate-source-map`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeneratedCrateSourceMap {
    /// Schema id (`GENERATED_CRATE_SOURCE_MAP_SCHEMA`).
    pub schema: SchemaId,
    /// Source document the map was derived from.
    pub source: String,
    /// Provenance entries in emission order.
    pub entries: Vec<GeneratedCrateSourceMapEntry>,
}

/// Parse a generated-crate provenance source map per
/// `emath.generated-crate-source-map`. Any other schema id is refused
/// (`E-EVID-108` class shape refusal): genesis bytes must never load as
/// the durable artifact source map.
pub fn generated_crate_source_map_from_json(
    json: &str,
) -> Result<GeneratedCrateSourceMap, ArtifactError> {
    let root = parse_json_document(json)?;
    let schema = SchemaId(root.string_field("schema")?);
    if schema.0 != GENERATED_CRATE_SOURCE_MAP_SCHEMA {
        return Err(ArtifactError::ManifestMalformed(format!(
            "schema is {}, expected {GENERATED_CRATE_SOURCE_MAP_SCHEMA}",
            schema.0
        )));
    }
    let source = root.string_field("source")?;
    let entries = match root.field("entries")? {
        JsonValue::Arr(items) => items
            .iter()
            .map(|item| {
                let kind = item.string_field("kind")?;
                if kind != "parametric-world" {
                    return Err(ArtifactError::ManifestMalformed(format!(
                        "generated-crate source-map entry kind is `{kind}`, expected `parametric-world`"
                    )));
                }
                Ok(GeneratedCrateSourceMapEntry {
                    generated: item.string_field("generated")?,
                    source: item.string_field("source")?,
                    kind,
                })
            })
            .collect::<Result<Vec<_>, ArtifactError>>()?,
        _ => return Err(ArtifactError::ManifestMalformed("bad entries".to_string())),
    };
    Ok(GeneratedCrateSourceMap {
        schema,
        source,
        entries,
    })
}

/// Serialize a resolution plan per `emath.resolution-plan`.
pub fn plan_to_record(plan: &ResolutionPlan) -> PlanRecord {
    let operations = plan
        .nodes
        .values()
        .map(|node| OperationRecord {
            node: u32::try_from(node.id.index()).unwrap_or(u32::MAX),
            operation: plan_operation_name(node),
            dependencies: node
                .dependencies
                .iter()
                .map(|id| u32::try_from(id.index()).unwrap_or(u32::MAX))
                .collect(),
            fallback: node
                .fallback
                .map(|id| u32::try_from(id.index()).unwrap_or(u32::MAX)),
        })
        .collect();
    PlanRecord {
        schema: SchemaId(RESOLUTION_PLAN_SCHEMA.to_string()),
        plan_id: plan.plan_id.clone(),
        goal: u32::try_from(plan.goal.index()).unwrap_or(u32::MAX),
        policy: plan.policy.clone(),
        artifact_class: plan.artifact_class.clone(),
        operations,
        excluded_candidates: plan
            .excluded_candidates
            .iter()
            .map(|candidate| (candidate.provider.clone(), candidate.reason.clone()))
            .collect(),
    }
}

pub(super) fn plan_operation_name(node: &PlanNodeDef) -> String {
    match node.operation {
        PlanOperation::Lower => "lower".to_string(),
        PlanOperation::Convert => "convert".to_string(),
        PlanOperation::Execute => "execute".to_string(),
        PlanOperation::Check => "check".to_string(),
        PlanOperation::Package => "package".to_string(),
        PlanOperation::Continue => "continue".to_string(),
        PlanOperation::ReturnUnresolved => "return-unresolved".to_string(),
        PlanOperation::Admit => "admit".to_string(),
    }
}

pub fn write_resolution_plan(plan: &PlanRecord) -> String {
    let operations = plan
        .operations
        .iter()
        .map(|op| {
            let deps = op
                .dependencies
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            let fallback = match op.fallback {
                Some(value) => value.to_string(),
                None => "null".to_string(),
            };
            let mut out = JsonWriter::object();
            out.int("node", u64::from(op.node));
            out.string("operation", &op.operation);
            out.field("dependencies", &format!("[{deps}]"));
            out.field("fallback", &fallback);
            out.finish()
        })
        .collect::<Vec<_>>()
        .join(",\n    ");
    let excluded = plan
        .excluded_candidates
        .iter()
        .map(|(provider, reason)| {
            let mut out = JsonWriter::object();
            out.string("provider", provider);
            out.string("reason", reason);
            out.finish()
        })
        .collect::<Vec<_>>()
        .join(",\n    ");
    let mut object = JsonWriter::object();
    object.field("schema", &quote("emath.resolution-plan"));
    object.field("plan_id", &content_id_or_empty(&plan.plan_id));
    object.int("goal", u64::from(plan.goal));
    object.string("policy", &plan.policy);
    object.string("artifact_class", &plan.artifact_class);
    object.field("operations", &format!("[\n    {operations}\n  ]"));
    object.field("excluded_candidates", &format!("[\n    {excluded}\n  ]"));
    object.finish()
}

pub(super) fn claim_json(claim: &EvidenceClaim) -> String {
    let mut out = JsonWriter::object();
    out.string("id", &claim.id);
    out.string("statement", &claim.statement);
    out.string("class", &claim.class);
    out.string("scope", &claim.scope);
    out.strings("assumptions", &claim.assumptions);
    out.string("producer", &claim.producer);
    out.string("checker", claim.checker.as_deref().unwrap_or(""));
    out.string("verdict", claim.verdict.as_str());
    out.string("level", claim.level.as_str());
    out.strings("falsifiers", &claim.falsifiers);
    out.strings("artifacts", &claim.artifacts);
    out.string("fresh_until", claim.fresh_until.as_deref().unwrap_or(""));
    out.finish()
}

/// Serialize an evidence bundle per `emath.evidence-bundle`.
pub fn write_evidence_bundle(bundle: &EvidenceBundleRecord) -> String {
    let claims = bundle
        .claims
        .iter()
        .map(claim_json)
        .collect::<Vec<_>>()
        .join(",\n    ");
    let mut object = JsonWriter::object();
    object.field("schema", &quote("emath.evidence-bundle"));
    object.field("bundle_id", &content_id_or_empty(&bundle.bundle_id));
    object.field(
        "source_package",
        &content_id_or_empty(&bundle.source_package),
    );
    object.field(
        "resolution_plan",
        &content_id_or_empty(&bundle.resolution_plan),
    );
    object.field("claims", &format!("[\n    {claims}\n  ]"));
    object.strings("artifact_paths", &bundle.artifact_paths);
    object.strings("reproduction", &bundle.reproduction);
    object.finish()
}
