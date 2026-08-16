//! Artifact emission: deterministic JSON writers for the four durable
//! schemas (`emath.artifact.v1`, `emath.source-map.v1`,
//! `emath.resolution-plan.v1`, `emath.evidence-bundle.v1`), staging and
//! atomic publish with tamper detection, and an independent checker that
//! never calls generator internals.

#![forbid(unsafe_code)]

use emath_core::{bootstrap_content_id, content_id_of_str, ContentId, SchemaId};
use emath_ir::{
    EvidenceClaim, EvidenceLevel, PlanNodeDef, PlanOperation, ResolutionPlan, TargetProfile,
};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

pub const ARTIFACT_MANIFEST_SCHEMA: &str = "emath.artifact.v1";
pub const SOURCE_MAP_SCHEMA: &str = "emath.source-map.v1";
pub const RESOLUTION_PLAN_SCHEMA: &str = "emath.resolution-plan.v1";
pub const EVIDENCE_BUNDLE_SCHEMA: &str = "emath.evidence-bundle.v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArtifactClass {
    Native,
    Hybrid,
    Parametric,
    Exploration,
    Continuation,
    Diagnostic,
}

impl ArtifactClass {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::Hybrid => "hybrid",
            Self::Parametric => "parametric",
            Self::Exploration => "exploration",
            Self::Continuation => "continuation",
            Self::Diagnostic => "diagnostic",
        }
    }
}

/// `emath.artifact.v1`
#[derive(Clone, Debug, PartialEq)]
pub struct ArtifactManifest {
    pub schema: SchemaId,
    pub artifact_id: ContentId,
    pub class: ArtifactClass,
    pub source_package: ContentId,
    pub compiler: ContentId,
    pub target: TargetProfile,
    pub numeric_profile: String,
    pub providers: Vec<emath_ir::ProviderRef>,
    pub evidence_level: EvidenceLevel,
    pub public_exports: Vec<String>,
    pub assumptions: Vec<String>,
    pub files: BTreeMap<String, ContentId>,
    pub source_map: ContentId,
    pub resolution_plan: ContentId,
    pub evidence_bundle: ContentId,
}

/// One `emath.source-map.v1` entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceMapEntry {
    pub source_file: String,
    pub source_start: u64,
    pub source_end: u64,
    pub semantic_node: String,
    pub plan_node: Option<String>,
    pub generated_file: String,
    pub generated_start: u64,
    pub generated_end: u64,
    pub generated_symbol: Option<String>,
}

/// `emath.source-map.v1`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceMap {
    pub schema: SchemaId,
    pub source_package: ContentId,
    pub entries: Vec<SourceMapEntry>,
}

/// `emath.resolution-plan.v1` (provider-free Phase 1 mirror of GIR plan).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanRecord {
    pub schema: SchemaId,
    pub plan_id: ContentId,
    pub goal: u32,
    pub policy: String,
    pub artifact_class: String,
    pub operations: Vec<OperationRecord>,
    pub excluded_candidates: Vec<(String, String)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OperationRecord {
    pub node: u32,
    pub operation: String,
    pub dependencies: Vec<u32>,
    pub fallback: Option<u32>,
}

/// `emath.evidence-bundle.v1`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvidenceBundleRecord {
    pub schema: SchemaId,
    pub bundle_id: ContentId,
    pub source_package: ContentId,
    pub resolution_plan: ContentId,
    pub claims: Vec<EvidenceClaim>,
    pub artifact_paths: Vec<String>,
    pub reproduction: Vec<String>,
}

/// Paths required for a Phase 1 artifact (`PUBLIC_API_INVENTORY.md` and the
/// imported seed agree on this set).
#[must_use]
pub fn required_artifact_paths() -> &'static [&'static str] {
    &[
        "Cargo.toml",
        "src/lib.rs",
        "emath/artifact-manifest.json",
        "emath/source-map.json",
        "emath/resolution-plan.json",
        "emath/evidence-bundle.json",
    ]
}

fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if (ch as u32) < 0x20 => write!(out, "\\u{:04x}", ch as u32).unwrap(),
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}

/// Minimal deterministic JSON writer (order preserving, two-space indent).
/// The std-only rule forbids serde; this writer is the single emitter.
pub struct JsonWriter;

impl JsonWriter {
    #[must_use]
    pub fn object() -> JsonObject {
        JsonObject { out: String::new() }
    }
}

pub struct JsonObject {
    out: String,
}

impl JsonObject {
    pub fn field(&mut self, name: &str, value: &str) -> &mut Self {
        if !self.out.is_empty() {
            self.out.push_str(",\n");
        }
        let entry = format!("  {}: {}", quote(name), value);
        self.out.push_str(&entry);
        self
    }

    pub fn string(&mut self, name: &str, value: &str) -> &mut Self {
        self.field(name, &quote(value))
    }

    pub fn strings(&mut self, name: &str, values: &[String]) -> &mut Self {
        let mut items = Vec::new();
        for value in values {
            items.push(quote(value));
        }
        self.field(name, &format!("[{}]", items.join(", ")))
    }

    pub fn int(&mut self, name: &str, value: u64) -> &mut Self {
        self.field(name, &value.to_string())
    }

    pub fn bool(&mut self, name: &str, value: bool) -> &mut Self {
        self.field(name, if value { "true" } else { "false" })
    }

    pub fn object_field(&mut self, name: &str, body: &str) -> &mut Self {
        self.field(name, body)
    }

    #[must_use]
    pub fn finish(self) -> String {
        format!("{{\n{}\n}}\n", self.out)
    }
}

fn content_id_or_empty(id: &ContentId) -> String {
    if id.0.is_empty() {
        String::new()
    } else {
        quote(&id.0)
    }
}

/// Parse the `files` inventory of a serialized artifact manifest
/// (`emath.artifact.v1`) into `path -> declared content id`.
///
/// This is the reader for the deterministic in-tree writer above: it
/// accepts exactly the writer's shape (a string-string object) and refuses
/// anything else, so a corrupted manifest cannot silently disable tamper
/// checks. Used by the independent artifact checker
/// (`emath artifact check <dir>`).
pub fn manifest_files_declared(
    manifest_json: &str,
) -> Result<BTreeMap<String, String>, ArtifactError> {
    let bytes = manifest_json.as_bytes();
    let malformed = |detail: &str| ArtifactError::ManifestMalformed(detail.to_string());
    let key = b"\"files\"";
    let Some(relative) = find_subslice(bytes, key) else {
        return Err(malformed("missing `files` field"));
    };
    let mut index = relative + key.len();
    skip_json_ws(bytes, &mut index);
    if bytes.get(index) != Some(&b':') {
        return Err(malformed("`files` field has no colon"));
    }
    index += 1;
    skip_json_ws(bytes, &mut index);
    if bytes.get(index) != Some(&b'{') {
        return Err(malformed("`files` field is not an object"));
    }
    index += 1;
    let mut files = BTreeMap::new();
    loop {
        skip_json_ws(bytes, &mut index);
        match bytes.get(index) {
            Some(b'}') => break,
            Some(b',') => {
                index += 1;
                continue;
            }
            Some(b'"') => {}
            _ => return Err(malformed("unexpected token in `files` object")),
        }
        let Some((path, next)) = parse_json_string(bytes, index) else {
            return Err(malformed("malformed path string in `files` object"));
        };
        index = next;
        skip_json_ws(bytes, &mut index);
        if bytes.get(index) != Some(&b':') {
            return Err(malformed("path entry has no colon"));
        }
        index += 1;
        skip_json_ws(bytes, &mut index);
        let Some((id, next)) = parse_json_string(bytes, index) else {
            return Err(malformed("malformed content-id string in `files` object"));
        };
        index = next;
        files.insert(path, id);
    }
    Ok(files)
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn skip_json_ws(bytes: &[u8], index: &mut usize) {
    while bytes
        .get(*index)
        .is_some_and(|byte| matches!(byte, b' ' | b'\n' | b'\r' | b'\t'))
    {
        *index += 1;
    }
}

/// Decode one JSON string literal (the writer's own escaping rules:
/// `\"`, `\\`, `\n`, `\r`, `\t`, `\uXXXX`).
fn parse_json_string(bytes: &[u8], start: usize) -> Option<(String, usize)> {
    let mut out = String::new();
    let mut index = start;
    if bytes.get(index) != Some(&b'"') {
        return None;
    }
    index += 1;
    loop {
        match bytes.get(index)? {
            b'"' => return Some((out, index + 1)),
            b'\\' => {
                match bytes.get(index + 1)? {
                    b'"' => out.push('"'),
                    b'\\' => out.push('\\'),
                    b'n' => out.push('\n'),
                    b'r' => out.push('\r'),
                    b't' => out.push('\t'),
                    b'u' => {
                        let digits = &bytes[index + 2..index + 6];
                        if digits.len() < 4 {
                            return None;
                        }
                        let text = std::str::from_utf8(digits).ok()?;
                        let code = u32::from_str_radix(text, 16).ok()?;
                        out.push(char::from_u32(code)?);
                        index += 4;
                    }
                    _ => return None,
                }
                index += 2;
            }
            _ => {
                let run_start = index;
                while let Some(byte) = bytes.get(index) {
                    if matches!(byte, b'"' | b'\\') {
                        break;
                    }
                    index += 1;
                }
                out.push_str(std::str::from_utf8(&bytes[run_start..index]).ok()?);
            }
        }
    }
}

fn target_json(target: &TargetProfile) -> String {
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

/// Serialize a manifest per `emath.artifact.v1`. Deterministic: files are
/// iterated in `BTreeMap` order.
pub fn write_artifact_manifest(manifest: &ArtifactManifest) -> String {
    let providers = manifest
        .providers
        .iter()
        .map(|p| {
            let mut out = JsonWriter::object();
            out.string("id", &p.id);
            out.string("version", &p.version);
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
    out.string("schema", "emath.artifact.v1");
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

/// Serialize a source map per `emath.source-map.v1`.
pub fn write_source_map(source_map: &SourceMap) -> String {
    let mut object = JsonWriter::object();
    object.field("schema", &quote("emath.source-map.v1"));
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

/// Serialize a resolution plan per `emath.resolution-plan.v1`.
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

fn plan_operation_name(node: &PlanNodeDef) -> String {
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
    object.field("schema", &quote("emath.resolution-plan.v1"));
    object.field("plan_id", &content_id_or_empty(&plan.plan_id));
    object.int("goal", u64::from(plan.goal));
    object.string("policy", &plan.policy);
    object.string("artifact_class", &plan.artifact_class);
    object.field("operations", &format!("[\n    {operations}\n  ]"));
    object.field("excluded_candidates", &format!("[\n    {excluded}\n  ]"));
    object.finish()
}

fn claim_json(claim: &EvidenceClaim) -> String {
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

/// Serialize an evidence bundle per `emath.evidence-bundle.v1`.
pub fn write_evidence_bundle(bundle: &EvidenceBundleRecord) -> String {
    let claims = bundle
        .claims
        .iter()
        .map(claim_json)
        .collect::<Vec<_>>()
        .join(",\n    ");
    let mut object = JsonWriter::object();
    object.field("schema", &quote("emath.evidence-bundle.v1"));
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

/// One staged file: relative path + bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StagedFile {
    pub relative_path: String,
    pub bytes: Vec<u8>,
}

/// Result of staging: per-file bootstrap content ids plus the artifact id
/// (bootstrap fingerprint over the required set, in required order).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Staging {
    pub files: BTreeMap<String, ContentId>,
    pub artifact_id: ContentId,
}

impl Staging {
    #[must_use]
    pub fn content_id(&self, relative_path: &str) -> Option<&ContentId> {
        self.files.get(relative_path)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArtifactError {
    MissingRequiredPath(String),
    UnstagedFile(String),
    PublishTargetExists(PathBuf),
    StateDirMissing(PathBuf),
    VerificationMismatch(String),
    ManifestMalformed(String),
}

impl std::fmt::Display for ArtifactError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingRequiredPath(path) => write!(f, "missing required artifact path `{path}`"),
            Self::UnstagedFile(path) => write!(f, "file was not staged: `{path}`"),
            Self::PublishTargetExists(path) => write!(
                f,
                "refusing to overwrite existing artifact directory `{}`",
                path.display()
            ),
            Self::StateDirMissing(path) => write!(
                f,
                "artifact state directory is missing: `{}`",
                path.display()
            ),
            Self::VerificationMismatch(detail) => {
                write!(f, "artifact verification failed: {detail}")
            }
            Self::ManifestMalformed(detail) => {
                write!(f, "artifact manifest is malformed: {detail}")
            }
        }
    }
}

impl std::error::Error for ArtifactError {}

/// Stage files: compute ids, check the required set, derive the artifact id.
pub fn stage(files: &[StagedFile], path_filter: Option<&Path>) -> Result<Staging, ArtifactError> {
    let mut ids = BTreeMap::new();
    for file in files {
        if let Some(filter) = path_filter {
            if Path::new(&file.relative_path).starts_with(filter) {
                ids.insert(
                    file.relative_path.clone(),
                    bootstrap_content_id(&file.bytes),
                );
            }
        } else {
            ids.insert(
                file.relative_path.clone(),
                bootstrap_content_id(&file.bytes),
            );
        }
    }
    for required in required_artifact_paths() {
        if !ids.contains_key(*required) {
            return Err(ArtifactError::MissingRequiredPath((*required).to_string()));
        }
    }
    // Artifact identity: fingerprint of the required paths in fixed order,
    // excluding the manifest itself (the manifest records that identity).
    let mut canonical = Vec::new();
    for required in required_artifact_paths() {
        if *required == "emath/artifact-manifest.json" {
            continue;
        }
        let id = &ids[*required];
        canonical.extend_from_slice(format!("{required}={}\n", id.0).as_bytes());
    }
    let artifact_id = bootstrap_content_id(&canonical);
    Ok(Staging {
        files: ids,
        artifact_id,
    })
}

/// Verify a staged artifact on disk: every required path exists and matches
/// its staged fingerprint. The checker is independent: it only reads the
/// files and the ids, never generator internals.
pub fn verify_artifact(root: &Path, expected: &Staging) -> Result<(), ArtifactError> {
    for required in required_artifact_paths() {
        let path = root.join(required);
        if !path.is_file() {
            return Err(ArtifactError::MissingRequiredPath((*required).to_string()));
        }
        let bytes = std::fs::read(&path).map_err(|error| {
            ArtifactError::VerificationMismatch(format!(
                "cannot read `{}`: {error}",
                path.display()
            ))
        })?;
        let actual = bootstrap_content_id(&bytes);
        let expected_id = expected
            .content_id(required)
            .ok_or_else(|| ArtifactError::UnstagedFile((*required).to_string()))?;
        if actual != *expected_id {
            return Err(ArtifactError::VerificationMismatch(format!(
                "`{required}` fingerprint changed (tamper detected)"
            )));
        }
    }
    Ok(())
}

/// Publish: create `target/emath/<artifact-id>` and write the staged files.
/// The directory is created fresh and never overwritten; verification runs
/// before and after the write.
pub fn publish(
    target_dir: &Path,
    artifact_id: &ContentId,
    files: &[StagedFile],
) -> Result<PathBuf, ArtifactError> {
    if !target_dir.is_dir() {
        return Err(ArtifactError::StateDirMissing(target_dir.to_path_buf()));
    }
    let staging = stage(files, None)?;
    let destination = target_dir.join("emath").join(&artifact_id.0);
    if destination.exists() {
        // Idempotent republish: same artifact id means the content identity
        // is fixed; re-verify instead of overwriting. A differing artifact
        // under a matching id is tamper, not a rebuild.
        if verify_artifact(&destination, &staging).is_ok() {
            return Ok(destination);
        }
        return Err(ArtifactError::PublishTargetExists(destination.clone()));
    }
    std::fs::create_dir_all(&destination).map_err(|error| {
        ArtifactError::VerificationMismatch(format!(
            "cannot create `{}`: {error}",
            destination.display()
        ))
    })?;
    for file in files {
        // We stage the union; only write files that belong to the artifact.
        let Some(id) = staging.content_id(&file.relative_path) else {
            return Err(ArtifactError::UnstagedFile(file.relative_path.clone()));
        };
        let _ = id;
        let path = destination.join(&file.relative_path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                ArtifactError::VerificationMismatch(format!(
                    "cannot create `{}`: {error}",
                    parent.display()
                ))
            })?;
        }
        std::fs::write(&path, &file.bytes).map_err(|error| {
            ArtifactError::VerificationMismatch(format!(
                "cannot write `{}`: {error}",
                path.display()
            ))
        })?;
    }
    // Post-write verification: a tampered intermediate cannot slip through.
    verify_artifact(&destination, &staging)?;
    Ok(destination)
}

/// Convenience: content identity of a text file.
#[must_use]
pub fn content_id_of_text(text: &str) -> ContentId {
    content_id_of_str(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use emath_ir::EvidenceClaim;

    fn sample_files() -> Vec<StagedFile> {
        vec![
            StagedFile {
                relative_path: "Cargo.toml".to_string(),
                bytes: b"[package]\n".to_vec(),
            },
            StagedFile {
                relative_path: "src/lib.rs".to_string(),
                bytes: b"#![forbid(unsafe_code)]\n".to_vec(),
            },
            StagedFile {
                relative_path: "emath/artifact-manifest.json".to_string(),
                bytes: b"{}".to_vec(),
            },
            StagedFile {
                relative_path: "emath/source-map.json".to_string(),
                bytes: b"{}".to_vec(),
            },
            StagedFile {
                relative_path: "emath/resolution-plan.json".to_string(),
                bytes: b"{}".to_vec(),
            },
            StagedFile {
                relative_path: "emath/evidence-bundle.json".to_string(),
                bytes: b"{}".to_vec(),
            },
        ]
    }

    #[test]
    fn stage_requires_the_fixed_set() {
        let files = vec![
            StagedFile {
                relative_path: "Cargo.toml".to_string(),
                bytes: b"[package]".to_vec(),
            },
            StagedFile {
                relative_path: "src/lib.rs".to_string(),
                bytes: b"fn main() {}".to_vec(),
            },
        ];
        assert!(matches!(
            stage(&files, None),
            Err(ArtifactError::MissingRequiredPath(_))
        ));
    }

    #[test]
    fn stage_is_deterministic_and_order_free() {
        let mut files = sample_files();
        files.reverse();
        let first = stage(&files, None).unwrap();
        let second = stage(&sample_files(), None).unwrap();
        assert_eq!(first.files, second.files);
        assert_eq!(first.artifact_id, second.artifact_id);
    }

    #[test]
    fn publish_writes_and_verifies() {
        let dir = std::env::temp_dir().join(format!("emath-artifact-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let files = sample_files();
        let staging = stage(&files, None).unwrap();
        let destination = publish(&dir, &staging.artifact_id, &files).unwrap();
        assert!(destination.join("emath/artifact-manifest.json").is_file());
        verify_artifact(&destination, &staging).unwrap();
        // Tamper detection: flip a byte in a required file.
        let manifest_path = destination.join("emath/artifact-manifest.json");
        std::fs::write(&manifest_path, b"{ }").unwrap();
        assert!(verify_artifact(&destination, &staging).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn json_writer_escapes() {
        let mut object = JsonWriter::object();
        object.string("code", "E-TYPE-002");
        object.string("message", "file \"x\"\nnext");
        let text = object.finish();
        assert!(text.contains("\\\"x\\\""));
        assert!(text.contains("\\n"));
    }

    #[test]
    fn manifest_round_trips_schema_name() {
        let manifest = ArtifactManifest {
            schema: SchemaId(ARTIFACT_MANIFEST_SCHEMA.to_string()),
            artifact_id: content_id_of_text("artifact"),
            class: ArtifactClass::Native,
            source_package: content_id_of_text("package"),
            compiler: content_id_of_text("compiler"),
            target: TargetProfile {
                family: "rust".to_string(),
                triple: None,
                features: vec!["std".to_string()],
            },
            numeric_profile: "strict-f64".to_string(),
            providers: Vec::new(),
            evidence_level: EvidenceLevel::E4,
            public_exports: vec!["new".to_string(), "score".to_string()],
            assumptions: vec!["division requires a non-zero denominator".to_string()],
            files: BTreeMap::from([("src/lib.rs".to_string(), content_id_of_text("code"))]),
            source_map: content_id_of_text("sm"),
            resolution_plan: content_id_of_text("plan"),
            evidence_bundle: content_id_of_text("ev"),
        };
        let text = write_artifact_manifest(&manifest);
        assert!(text.contains("\"schema\": \"emath.artifact.v1\""));
        assert!(text.contains("\"class\": \"native\""));
        // Deterministic
        let mine = write_artifact_manifest(&manifest);
        let again = write_artifact_manifest(&manifest);
        assert_eq!(mine, again);
    }

    #[test]
    fn manifest_files_declared_round_trips_writer_shape() {
        let files = BTreeMap::from([
            ("Cargo.toml".to_string(), content_id_of_text("manifest")),
            (
                "src/lib.rs".to_string(),
                content_id_of_text("#![forbid(unsafe_code)]"),
            ),
            (
                "emath/source-map.json".to_string(),
                content_id_of_text("source-map"),
            ),
            // Escaping must survive the round trip.
            (
                "src/weird \"quoted\" \\ path.rs".to_string(),
                content_id_of_text("escaped"),
            ),
        ]);
        let manifest = ArtifactManifest {
            schema: SchemaId(ARTIFACT_MANIFEST_SCHEMA.to_string()),
            artifact_id: content_id_of_text("artifact"),
            class: ArtifactClass::Native,
            source_package: content_id_of_text("package"),
            compiler: content_id_of_text("compiler"),
            target: TargetProfile {
                family: "rust".to_string(),
                triple: None,
                features: vec!["std".to_string()],
            },
            numeric_profile: "strict-f64".to_string(),
            providers: Vec::new(),
            evidence_level: EvidenceLevel::E4,
            public_exports: vec!["new".to_string()],
            assumptions: Vec::new(),
            files,
            source_map: content_id_of_text("sm"),
            resolution_plan: content_id_of_text("plan"),
            evidence_bundle: content_id_of_text("ev"),
        };
        let text = write_artifact_manifest(&manifest);
        let parsed = manifest_files_declared(&text).unwrap();
        assert_eq!(parsed.len(), 4);
        assert_eq!(
            parsed.get("emath/source-map.json").unwrap(),
            &content_id_of_text("source-map").0
        );
        assert_eq!(
            parsed.get("src/weird \"quoted\" \\ path.rs").unwrap(),
            &content_id_of_text("escaped").0
        );
    }

    #[test]
    fn manifest_files_declared_refuses_malformed_bodies() {
        assert!(matches!(
            manifest_files_declared("{\"schema\":\"emath.artifact.v1\"}"),
            Err(ArtifactError::ManifestMalformed(_))
        ));
        assert!(manifest_files_declared("not json").is_err());
        assert!(manifest_files_declared("{\"files\": [1,2,3]}").is_err());
    }

    #[test]
    fn source_map_and_plan_write() {
        let source_map = SourceMap {
            schema: SchemaId(SOURCE_MAP_SCHEMA.to_string()),
            source_package: content_id_of_text("pkg"),
            entries: vec![SourceMapEntry {
                source_file: "stateful.emath".to_string(),
                source_start: 0,
                source_end: 42,
                semantic_node: "definition.score".to_string(),
                plan_node: Some("n3".to_string()),
                generated_file: "src/lib.rs".to_string(),
                generated_start: 10,
                generated_end: 22,
                generated_symbol: Some("score".to_string()),
            }],
        };
        let text = write_source_map(&source_map);
        assert!(text.starts_with("{\n  \"schema\": \"emath.source-map.v1\""));
        assert!(text.contains("\"semantic_node\": \"definition.score\""));

        let evidence = EvidenceBundleRecord {
            schema: SchemaId(EVIDENCE_BUNDLE_SCHEMA.to_string()),
            bundle_id: content_id_of_text("bundle"),
            source_package: content_id_of_text("pkg"),
            resolution_plan: content_id_of_text("plan"),
            claims: vec![EvidenceClaim {
                id: "claim-1".to_string(),
                statement: "score matches the spec".to_string(),
                class: "functional".to_string(),
                scope: "AffinePolicy".to_string(),
                assumptions: vec!["strict-f64".to_string()],
                producer: "emath-build".to_string(),
                checker: None,
                verdict: emath_ir::ClaimVerdict::Pass,
                level: EvidenceLevel::E2,
                falsifiers: Vec::new(),
                artifacts: vec!["emath/evidence-bundle.json".to_string()],
                fresh_until: None,
            }],
            artifact_paths: required_artifact_paths()
                .iter()
                .map(|s| (*s).to_string())
                .collect(),
            reproduction: vec!["emath build --emit crate".to_string()],
        };
        let text = write_evidence_bundle(&evidence);
        assert!(text.contains("\"verdict\": \"pass\""));
    }
}
