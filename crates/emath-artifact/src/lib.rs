//! Artifact emission: deterministic JSON writers for the four durable
//! schemas (`emath.artifact`, `emath.source-map`,
//! `emath.resolution-plan`, `emath.evidence-bundle`), staging and
//! atomic publish with content-identity verification, and an independent checker that
//! never calls generator internals.

#![forbid(unsafe_code)]

use emath_core::{bootstrap_content_id, content_id_of_str, fnv1a64_bytes, ContentId, SchemaId};
use emath_ir::{
    ClaimVerdict, EvidenceClaim, EvidenceLevel, PlanNodeDef, PlanOperation, ResolutionPlan,
    TargetProfile,
};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

pub const ARTIFACT_MANIFEST_SCHEMA: &str = "emath.artifact";
/// Artifact manifest document version (manifest v1). Bump on any change
/// to the manifest layout or to the identity preimage in
/// [`manifest_identity`]; consumers refuse versions they do not know.
pub const ARTIFACT_MANIFEST_VERSION: u32 = 1;
/// Durable artifact source map (byte-range + `source_package` shape; see
/// [`write_source_map`]). Distinct from the world-codegen provenance map
/// ([`GENERATED_CRATE_SOURCE_MAP_SCHEMA`]); the two never share an id.
pub const SOURCE_MAP_SCHEMA: &str = "emath.source-map";
/// World-codegen provenance map written next to a generated world crate
/// (see [`write_generated_crate_source_map`]). Distinct from
/// [`SOURCE_MAP_SCHEMA`]; the two documents must never share an id.
pub const GENERATED_CRATE_SOURCE_MAP_SCHEMA: &str = "emath.generated-crate-source-map";
/// JSON `$schema` id of the durable resolution-plan document
/// ([`write_resolution_plan`]). The plan identity preimage is
/// `plan_identity` over a `plan:` payload, not this document id.
pub const RESOLUTION_PLAN_SCHEMA: &str = "emath.resolution-plan";
pub const EVIDENCE_BUNDLE_SCHEMA: &str = "emath.evidence-bundle";

/// The seven total-artifact-protocol classes. Compilation is total over
/// this set: every accepted intent resolves to an artifact of some class,
/// and resolution monotonicity requires that adding providers or budgets
/// never destroys a class that was previously reachable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArtifactClass {
    Native,
    Portfolio,
    Hybrid,
    Parametric,
    Exploration,
    Continuation,
    Diagnostic,
}

impl ArtifactClass {
    /// All seven classes in stable protocol order.
    pub const ALL: [Self; 7] = [
        Self::Native,
        Self::Portfolio,
        Self::Hybrid,
        Self::Parametric,
        Self::Exploration,
        Self::Continuation,
        Self::Diagnostic,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::Portfolio => "portfolio",
            Self::Hybrid => "hybrid",
            Self::Parametric => "parametric",
            Self::Exploration => "exploration",
            Self::Continuation => "continuation",
            Self::Diagnostic => "diagnostic",
        }
    }
}

impl std::str::FromStr for ArtifactClass {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "native" => Ok(Self::Native),
            "portfolio" => Ok(Self::Portfolio),
            "hybrid" => Ok(Self::Hybrid),
            "parametric" => Ok(Self::Parametric),
            "exploration" => Ok(Self::Exploration),
            "continuation" => Ok(Self::Continuation),
            "diagnostic" => Ok(Self::Diagnostic),
            _ => Err(()),
        }
    }
}

/// `emath.artifact`
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

/// One `emath.source-map` entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceMapEntry {
    /// Source file id (index into the session source store).
    pub file: u32,
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

/// `emath.source-map`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceMap {
    pub schema: SchemaId,
    pub source_package: ContentId,
    pub entries: Vec<SourceMapEntry>,
}

/// `emath.resolution-plan` (provider-free Phase 1 mirror of GIR plan).
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

/// `emath.evidence-bundle`
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

/// The four metadata documents every artifact package carries regardless
/// of class: the durable manifest, source map, resolution plan and
/// evidence bundle.
const METADATA_PATHS: [&str; 4] = [
    "emath/artifact-manifest.json",
    "emath/source-map.json",
    "emath/resolution-plan.json",
    "emath/evidence-bundle.json",
];

/// Paths required for a Phase 1 artifact; per-class inventories live on
/// [`required_paths_for_class`].
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

/// Package contents per artifact class: code-bearing classes (native,
/// portfolio, hybrid, parametric, continuation) ship a Cargo crate plus the
/// four metadata documents; exploration/diagnostic artifacts are
/// metadata-only.
#[must_use]
pub fn required_paths_for_class(class: ArtifactClass) -> &'static [&'static str] {
    match class {
        ArtifactClass::Native
        | ArtifactClass::Portfolio
        | ArtifactClass::Hybrid
        | ArtifactClass::Parametric
        | ArtifactClass::Continuation => required_artifact_paths(),
        ArtifactClass::Exploration | ArtifactClass::Diagnostic => &METADATA_PATHS,
    }
}

/// The sole artifact identity: deterministic hash of the manifest body,
/// excluding the self-referential `artifact_id` and content-id entries.
/// The only identity the publisher records and the checker recomputes
/// (`E-EVID-102`).
#[must_use]
pub fn manifest_identity(manifest: &ArtifactManifest) -> ContentId {
    let mut files: Vec<(String, &ContentId)> = manifest
        .files
        .iter()
        .filter(|(path, _)| *path != "emath/artifact-manifest.json")
        .map(|(path, id)| (path.clone(), id))
        .collect();
    files.sort_by(|left, right| left.0.cmp(&right.0));
    let file_token: Vec<String> = files
        .iter()
        .map(|(path, id)| format!("{path}={}", id.0))
        .collect();
    let mut providers = manifest.providers.clone();
    providers.sort_by(|left, right| left.id.cmp(&right.id));
    let provider_token: Vec<String> = providers
        .iter()
        .map(|p| {
            format!(
                "{}@{}:{}-{}",
                p.id, p.version, p.implementation.0, manifest.numeric_profile
            )
        })
        .collect();
    let body = format!(
        "artifact:{}:{}:{}:{}:{}:{}:[{}]:[{}]:{}:{}:{}",
        manifest.schema.0,
        manifest.source_package.0,
        manifest.class.as_str(),
        manifest.compiler.0,
        manifest.target.family,
        manifest.target.triple.as_deref().unwrap_or("-"),
        file_token.join(";"),
        provider_token.join(";"),
        manifest.source_map.0,
        manifest.resolution_plan.0,
        manifest.evidence_bundle.0,
    );
    ContentId(format!("fnv1a64:{:016x}", fnv1a64_bytes(body.as_bytes())))
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
            ch if (ch as u32) < 0x20 => {
                // fmt::Write for String is infallible; avoid unwrap on the hot path.
                let _ = write!(out, "\\u{:04x}", ch as u32);
            }
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

    /// Array of already-serialized JSON objects. `items` are `finish()` bodies
    /// (or other object texts); this crate owns the array brackets so callers
    /// do not concatenate JSON by hand.
    pub fn objects(&mut self, name: &str, items: &[String]) -> &mut Self {
        if items.is_empty() {
            return self.field(name, "[]");
        }
        let mut body = String::from("[\n");
        for (index, item) in items.iter().enumerate() {
            if index > 0 {
                body.push_str(",\n");
            }
            body.push_str(item.trim());
        }
        body.push_str("\n  ]");
        self.field(name, &body)
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

/// Serialize an id field; an unresolved (empty) id still must produce a
/// valid JSON string, otherwise `"field": ` would be emitted and no
/// reader could ever parse the document (documents are read back).
fn content_id_or_empty(id: &ContentId) -> String {
    if id.0.is_empty() {
        quote("")
    } else {
        quote(&id.0)
    }
}

/// Parse the `files` inventory of a serialized artifact manifest into
/// `path -> declared content id`. Accepts exactly the writer's shape;
/// anything else is refused, so a corrupted manifest cannot disable
/// content-identity verification.
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

// ---------------------------------------------------------------------------
// Reader side: the writer's documents are parsed back with the
// same std-only discipline, so the CLI and checker never rely on
// write-only artifacts.
// ---------------------------------------------------------------------------

/// Minimal JSON value tree accepted by [`parse_json_document`].
#[derive(Clone, Debug, PartialEq)]
pub enum JsonValue {
    /// String literal.
    Str(String),
    /// Numeric literal (kept verbatim).
    Num(String),
    /// Boolean literal.
    Bool(bool),
    /// `null`.
    Null,
    /// Object (insertion order preserved).
    Obj(Vec<(String, JsonValue)>),
    /// Array.
    Arr(Vec<JsonValue>),
}

impl JsonValue {
    /// Look up an object field by name (typed parse-back support).
    pub fn field(&self, name: &str) -> Result<&JsonValue, ArtifactError> {
        match self {
            Self::Obj(entries) => entries
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value)
                .ok_or_else(|| ArtifactError::ManifestMalformed(format!("missing `{name}`"))),
            _ => Err(ArtifactError::ManifestMalformed(
                "not an object".to_string(),
            )),
        }
    }

    /// Read a string field (typed parse-back support).
    pub fn string_field(&self, name: &str) -> Result<String, ArtifactError> {
        match self.field(name)? {
            Self::Str(value) => Ok(value.clone()),
            _ => Err(ArtifactError::ManifestMalformed(format!(
                "`{name}` is not a string"
            ))),
        }
    }

    fn optional_string_field(&self, name: &str) -> Result<Option<String>, ArtifactError> {
        if self.obj_has(name)? {
            Ok(Some(self.string_field(name)?))
        } else {
            Ok(None)
        }
    }

    fn obj_has(&self, name: &str) -> Result<bool, ArtifactError> {
        match self {
            Self::Obj(entries) => Ok(entries.iter().any(|(key, _)| key == name)),
            _ => Err(ArtifactError::ManifestMalformed(
                "not an object".to_string(),
            )),
        }
    }

    /// Read an integer field (typed parse-back support).
    pub fn int_field(&self, name: &str) -> Result<u64, ArtifactError> {
        match self.field(name)? {
            Self::Num(value) => value.parse::<u64>().map_err(|_| {
                ArtifactError::ManifestMalformed(format!("`{name}` is not an integer"))
            }),
            _ => Err(ArtifactError::ManifestMalformed(format!(
                "`{name}` is not a number"
            ))),
        }
    }

    fn strings_field(&self, name: &str) -> Result<Vec<String>, ArtifactError> {
        match self.field(name)? {
            Self::Arr(items) => items
                .iter()
                .map(|item| match item {
                    Self::Str(value) => Ok(value.clone()),
                    _ => Err(ArtifactError::ManifestMalformed(format!(
                        "`{name}` array has a non-string entry"
                    ))),
                })
                .collect(),
            _ => Err(ArtifactError::ManifestMalformed(format!(
                "`{name}` is not an array"
            ))),
        }
    }

    fn content_id_field(&self, name: &str) -> Result<ContentId, ArtifactError> {
        Ok(ContentId(self.string_field(name)?))
    }
}

/// Parse one deterministic writer document into a value tree. Accepts the
/// writer's grammar (RFC 8259 subset: objects, arrays, strings, integers,
/// booleans and `null`); anything else is a typed refusal.
pub fn parse_json_document(text: &str) -> Result<JsonValue, ArtifactError> {
    let bytes = text.as_bytes();
    let mut index = 0;
    let value = parse_json_value(bytes, &mut index)
        .ok_or_else(|| ArtifactError::ManifestMalformed("cannot parse JSON".to_string()))?;
    skip_json_ws(bytes, &mut index);
    if index != bytes.len() {
        return Err(ArtifactError::ManifestMalformed(
            "trailing content after JSON document".to_string(),
        ));
    }
    Ok(value)
}

fn parse_json_value(bytes: &[u8], index: &mut usize) -> Option<JsonValue> {
    skip_json_ws(bytes, index);
    match bytes.get(*index) {
        Some(&b'{') => parse_json_object(bytes, index).map(JsonValue::Obj),
        Some(&b'[') => parse_json_array(bytes, index).map(JsonValue::Arr),
        Some(&b'"') => parse_json_string(bytes, *index).map(|(value, next)| {
            *index = next;
            JsonValue::Str(value)
        }),
        Some(&b't')
            if bytes.get(*index + 1) == Some(&b'r')
                && bytes.get(*index + 2) == Some(&b'u')
                && bytes.get(*index + 3) == Some(&b'e') =>
        {
            *index += 4;
            Some(JsonValue::Bool(true))
        }
        Some(&b'f')
            if bytes.get(*index + 1) == Some(&b'a')
                && bytes.get(*index + 2) == Some(&b'l')
                && bytes.get(*index + 3) == Some(&b's')
                && bytes.get(*index + 4) == Some(&b'e') =>
        {
            *index += 5;
            Some(JsonValue::Bool(false))
        }
        Some(&b'n')
            if bytes.get(*index + 1) == Some(&b'u')
                && bytes.get(*index + 2) == Some(&b'l')
                && bytes.get(*index + 3) == Some(&b'l') =>
        {
            *index += 4;
            Some(JsonValue::Null)
        }
        Some(&(b'-' | b'0'..=b'9')) => {
            let start = *index;
            while bytes
                .get(*index)
                .is_some_and(|byte| matches!(byte, b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E'))
            {
                *index += 1;
            }
            Some(JsonValue::Num(
                std::str::from_utf8(&bytes[start..*index]).ok()?.to_string(),
            ))
        }
        _ => None,
    }
}

fn parse_json_object(bytes: &[u8], index: &mut usize) -> Option<Vec<(String, JsonValue)>> {
    *index += 1; // '{'
    let mut entries = Vec::new();
    loop {
        skip_json_ws(bytes, index);
        match bytes.get(*index)? {
            b'}' => {
                *index += 1;
                return Some(entries);
            }
            b'"' => {}
            _ => return None,
        }
        let (key, next) = parse_json_string(bytes, *index)?;
        *index = next;
        skip_json_ws(bytes, index);
        if bytes.get(*index) != Some(&b':') {
            return None;
        }
        *index += 1;
        let value = parse_json_value(bytes, index)?;
        entries.push((key, value));
        skip_json_ws(bytes, index);
        match bytes.get(*index)? {
            b',' => *index += 1,
            b'}' => {
                *index += 1;
                return Some(entries);
            }
            _ => return None,
        }
    }
}

fn parse_json_array(bytes: &[u8], index: &mut usize) -> Option<Vec<JsonValue>> {
    *index += 1; // '['
    let mut items = Vec::new();
    loop {
        skip_json_ws(bytes, index);
        if bytes.get(*index) == Some(&b']') {
            *index += 1;
            return Some(items);
        }
        items.push(parse_json_value(bytes, index)?);
        skip_json_ws(bytes, index);
        match bytes.get(*index)? {
            b',' => *index += 1,
            b']' => {
                *index += 1;
                return Some(items);
            }
            _ => return None,
        }
    }
}

/// Parse a manifest per `emath.artifact` (field shape of
/// [`write_artifact_manifest`]).
pub fn manifest_from_json(json: &str) -> Result<ArtifactManifest, ArtifactError> {
    let root = parse_json_document(json)?;
    let class = root.string_field("class")?;
    let class = class
        .parse::<ArtifactClass>()
        .map_err(|()| ArtifactError::ManifestMalformed("unknown artifact class".to_string()))?;
    let level = root.string_field("evidence_level")?;
    let level = level
        .parse::<EvidenceLevel>()
        .map_err(|()| ArtifactError::ManifestMalformed("unknown evidence level".to_string()))?;
    let target = root.field("target")?;
    let triple = match target.field("triple")? {
        JsonValue::Null => None,
        JsonValue::Str(value) if value.is_empty() => None,
        JsonValue::Str(value) => Some(value.clone()),
        _ => {
            return Err(ArtifactError::ManifestMalformed(
                "bad target triple".to_string(),
            ));
        }
    };
    let providers = match root.field("providers")? {
        JsonValue::Arr(items) => items
            .iter()
            .map(|item| {
                Ok(emath_ir::ProviderRef {
                    id: item.string_field("id")?,
                    version: item.string_field("version")?,
                    implementation: item.content_id_field("implementation")?,
                })
            })
            .collect::<Result<Vec<_>, ArtifactError>>()?,
        _ => {
            return Err(ArtifactError::ManifestMalformed(
                "bad providers".to_string(),
            ));
        }
    };
    let files = match root.field("files")? {
        JsonValue::Obj(entries) => entries
            .iter()
            .map(|(path, id)| match id {
                JsonValue::Str(value) => Ok((path.clone(), ContentId(value.clone()))),
                _ => Err(ArtifactError::ManifestMalformed(
                    "bad file content id".to_string(),
                )),
            })
            .collect::<Result<BTreeMap<_, _>, ArtifactError>>()?,
        _ => return Err(ArtifactError::ManifestMalformed("bad files".to_string())),
    };
    Ok(ArtifactManifest {
        schema: SchemaId(root.string_field("schema")?),
        artifact_id: root.content_id_field("artifact_id")?,
        class,
        source_package: root.content_id_field("source_package")?,
        compiler: root.content_id_field("compiler")?,
        target: TargetProfile {
            family: target.string_field("family")?,
            triple,
            features: target.strings_field("features")?,
        },
        numeric_profile: root.string_field("numeric_profile")?,
        providers,
        evidence_level: level,
        public_exports: root.strings_field("public_exports")?,
        assumptions: root.strings_field("assumptions")?,
        files,
        source_map: root.content_id_field("source_map")?,
        resolution_plan: root.content_id_field("resolution_plan")?,
        evidence_bundle: root.content_id_field("evidence_bundle")?,
    })
}

/// Parse a source map per `emath.source-map` (field shape of
/// [`write_source_map`]); empty `plan_node`/`generated_symbol` strings
/// round-trip as `None`.
pub fn source_map_from_json(json: &str) -> Result<SourceMap, ArtifactError> {
    let root = parse_json_document(json)?;
    let entries = match root.field("entries")? {
        JsonValue::Arr(items) => items
            .iter()
            .map(|item| {
                Ok(SourceMapEntry {
                    file: u32::try_from(item.int_field("file")?).map_err(|_| {
                        ArtifactError::ManifestMalformed("`file` is not a u32".to_string())
                    })?,
                    source_file: item.string_field("source_file")?,
                    source_start: item.int_field("source_start")?,
                    source_end: item.int_field("source_end")?,
                    semantic_node: item.string_field("semantic_node")?,
                    plan_node: item
                        .optional_string_field("plan_node")?
                        .filter(|value| !value.is_empty()),
                    generated_file: item.string_field("generated_file")?,
                    generated_start: item.int_field("generated_start")?,
                    generated_end: item.int_field("generated_end")?,
                    generated_symbol: item
                        .optional_string_field("generated_symbol")?
                        .filter(|value| !value.is_empty()),
                })
            })
            .collect::<Result<Vec<_>, ArtifactError>>()?,
        _ => return Err(ArtifactError::ManifestMalformed("bad entries".to_string())),
    };
    Ok(SourceMap {
        schema: SchemaId(root.string_field("schema")?),
        source_package: root.content_id_field("source_package")?,
        entries,
    })
}

/// Parse a resolution plan per `emath.resolution-plan` (schema and
/// plan identity are the checked surface).
pub fn plan_from_json(json: &str) -> Result<PlanRecord, ArtifactError> {
    let root = parse_json_document(json)?;
    let operations = match root.field("operations")? {
        JsonValue::Arr(items) => items
            .iter()
            .map(|item| {
                let fallback = match item.field("fallback")? {
                    JsonValue::Null => None,
                    JsonValue::Num(value) => Some(value.parse::<u32>().map_err(|_| {
                        ArtifactError::ManifestMalformed("bad fallback".to_string())
                    })?),
                    _ => {
                        return Err(ArtifactError::ManifestMalformed("bad fallback".to_string()));
                    }
                };
                let dependencies = match item.field("dependencies")? {
                    JsonValue::Arr(items) => items
                        .iter()
                        .map(|dep| match dep {
                            JsonValue::Num(value) => value.parse::<u32>().map_err(|_| {
                                ArtifactError::ManifestMalformed("bad dependency".to_string())
                            }),
                            _ => Err(ArtifactError::ManifestMalformed(
                                "bad dependency".to_string(),
                            )),
                        })
                        .collect::<Result<Vec<_>, ArtifactError>>()?,
                    _ => {
                        return Err(ArtifactError::ManifestMalformed(
                            "bad dependencies".to_string(),
                        ));
                    }
                };
                Ok(OperationRecord {
                    node: item
                        .int_field("node")?
                        .try_into()
                        .map_err(|_| ArtifactError::ManifestMalformed("bad node".to_string()))?,
                    operation: item.string_field("operation")?,
                    dependencies,
                    fallback,
                })
            })
            .collect::<Result<Vec<_>, ArtifactError>>()?,
        _ => {
            return Err(ArtifactError::ManifestMalformed(
                "bad operations".to_string(),
            ));
        }
    };
    let excluded_candidates = match root.field("excluded_candidates")? {
        JsonValue::Arr(items) => items
            .iter()
            .map(|item| Ok((item.string_field("provider")?, item.string_field("reason")?)))
            .collect::<Result<Vec<_>, ArtifactError>>()?,
        _ => {
            return Err(ArtifactError::ManifestMalformed(
                "bad excluded_candidates".to_string(),
            ));
        }
    };
    Ok(PlanRecord {
        schema: SchemaId(root.string_field("schema")?),
        plan_id: root.content_id_field("plan_id")?,
        goal: root
            .int_field("goal")?
            .try_into()
            .map_err(|_| ArtifactError::ManifestMalformed("goal does not fit u32".to_string()))?,
        policy: root.string_field("policy")?,
        artifact_class: root.string_field("artifact_class")?,
        operations,
        excluded_candidates,
    })
}

/// Parse an evidence bundle per `emath.evidence-bundle`; empty
/// `checker`/`fresh_until` strings round-trip as `None`.
pub fn evidence_bundle_from_json(json: &str) -> Result<EvidenceBundleRecord, ArtifactError> {
    let root = parse_json_document(json)?;
    let claims = match root.field("claims")? {
        JsonValue::Arr(items) => items
            .iter()
            .map(|item| {
                let verdict = item
                    .string_field("verdict")?
                    .parse::<ClaimVerdict>()
                    .map_err(|()| ArtifactError::ManifestMalformed("bad verdict".to_string()))?;
                let checker = item
                    .optional_string_field("checker")?
                    .filter(|value| !value.is_empty());
                let fresh_until = item
                    .optional_string_field("fresh_until")?
                    .filter(|value| !value.is_empty());
                let level = item
                    .string_field("level")?
                    .parse::<EvidenceLevel>()
                    .map_err(|()| {
                        ArtifactError::ManifestMalformed("bad claim level".to_string())
                    })?;
                Ok(EvidenceClaim {
                    id: item.string_field("id")?,
                    statement: item.string_field("statement")?,
                    class: item.string_field("class")?,
                    scope: item.string_field("scope")?,
                    assumptions: item.strings_field("assumptions")?,
                    producer: item.string_field("producer")?,
                    checker,
                    verdict,
                    level,
                    falsifiers: item.strings_field("falsifiers")?,
                    artifacts: item.strings_field("artifacts")?,
                    fresh_until,
                })
            })
            .collect::<Result<Vec<_>, ArtifactError>>()?,
        _ => return Err(ArtifactError::ManifestMalformed("bad claims".to_string())),
    };
    Ok(EvidenceBundleRecord {
        schema: SchemaId(root.string_field("schema")?),
        bundle_id: root.content_id_field("bundle_id")?,
        source_package: root.content_id_field("source_package")?,
        resolution_plan: root.content_id_field("resolution_plan")?,
        claims,
        artifact_paths: root.strings_field("artifact_paths")?,
        reproduction: root.strings_field("reproduction")?,
    })
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
    let mut entries = String::from("[");
    for (index, rel) in files.iter().enumerate() {
        if index > 0 {
            entries.push(',');
        }
        let _ = write!(
            entries,
            "{{\"generated\":\"{rel}\",\"source\":\"{source}\",\"kind\":\"parametric-world\"}}"
        );
    }
    entries.push(']');
    object.object_field("entries", &entries);
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
    object.field("schema", &quote("emath.resolution-plan"));
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
    StateDirMissing(PathBuf),
    VerificationMismatch(String),
    ManifestMalformed(String),
    InvalidStagedPath(String),
}

impl std::fmt::Display for ArtifactError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingRequiredPath(path) => write!(f, "missing required artifact path `{path}`"),
            Self::UnstagedFile(path) => write!(f, "file was not staged: `{path}`"),
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
            Self::InvalidStagedPath(path) => {
                write!(f, "refusing unsafe staged path `{path}`")
            }
        }
    }
}

impl std::error::Error for ArtifactError {}

/// Whether `path` exists and is a symlink (publish and verify
/// refuse to follow links, so a link cannot smuggle files in or out of
/// the artifact destination).
fn path_is_symlink(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink())
}

/// Refuses absolute staged paths and `..` traversal components: every
/// staged path must stay inside the artifact destination.
fn check_relative_path(relative_path: &str) -> Result<(), ArtifactError> {
    let path = std::path::Path::new(relative_path);
    if path.is_absolute() {
        return Err(ArtifactError::InvalidStagedPath(relative_path.to_string()));
    }
    for component in path.components() {
        if component == std::path::Component::ParentDir {
            return Err(ArtifactError::InvalidStagedPath(relative_path.to_string()));
        }
    }
    Ok(())
}

/// Stage files: compute ids, check the required set, derive the artifact id.
pub fn stage(files: &[StagedFile], path_filter: Option<&Path>) -> Result<Staging, ArtifactError> {
    let mut ids = BTreeMap::new();
    for file in files {
        check_relative_path(&file.relative_path)?;
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
        if path_is_symlink(&path) {
            return Err(ArtifactError::VerificationMismatch(format!(
                "`{required}` is a symlink"
            )));
        }
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
                "`{required}` fingerprint changed (content-identity mismatch)"
            )));
        }
    }
    Ok(())
}

/// Publish: create `target/emath/<artifact-id>` and write the staged files.
/// The destination is created atomically (temporary sibling dir, post-write
/// verification, rename); verification runs before and after the write.
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
    if path_is_symlink(target_dir) || path_is_symlink(&target_dir.join("emath")) {
        return Err(ArtifactError::VerificationMismatch(
            "refusing to publish through a symlinked state directory".to_string(),
        ));
    }
    if destination.exists() {
        // Idempotent republish: same artifact id means the content identity
        // is fixed; re-verify instead of overwriting. A verification
        // failure here is tamper/corruption, not a rebuild collision: the
        // typed mismatch is returned, never collapsed into "target exists".
        if verify_artifact(&destination, &staging).is_ok() {
            return Ok(destination);
        }
        return Err(ArtifactError::VerificationMismatch(format!(
            "existing artifact at `{}` failed content-identity verification (tampered or corrupted; not a rebuild)",
            destination.display()
        )));
    }
    // Atomic publish: everything is written under a temporary sibling
    // directory and renamed into place only after post-write verification
    // succeeds. A failure or crash leaves no destination directory and a
    // retry starts from a clean slate.
    let emath_root = target_dir.join("emath");
    let staging_dir = emath_root.join(format!(".tmp-{}", artifact_id.0));
    let _ = std::fs::remove_dir_all(&staging_dir);
    if let Err(error) = std::fs::create_dir_all(&staging_dir) {
        let _ = std::fs::remove_dir_all(&staging_dir);
        return Err(ArtifactError::VerificationMismatch(format!(
            "cannot create staging `{}`: {error}",
            staging_dir.display(),
        )));
    }
    for file in files {
        // We stage the union; only write files that belong to the artifact.
        let Some(id) = staging.content_id(&file.relative_path) else {
            let _ = std::fs::remove_dir_all(&staging_dir);
            return Err(ArtifactError::UnstagedFile(file.relative_path.clone()));
        };
        let _ = id;
        let path = staging_dir.join(&file.relative_path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                let _ = std::fs::remove_dir_all(&staging_dir);
                ArtifactError::VerificationMismatch(format!(
                    "cannot create `{}`: {error}",
                    parent.display(),
                ))
            })?;
        }
        if path_is_symlink(&path) {
            let _ = std::fs::remove_dir_all(&staging_dir);
            return Err(ArtifactError::VerificationMismatch(format!(
                "refusing to write through symlink `{}`",
                path.display(),
            )));
        }
        std::fs::write(&path, &file.bytes).map_err(|error| {
            let _ = std::fs::remove_dir_all(&staging_dir);
            ArtifactError::VerificationMismatch(format!(
                "cannot write `{}`: {error}",
                path.display(),
            ))
        })?;
    }
    // Post-write verification: a mismatched intermediate cannot slip
    // through to the published tree.
    if let Err(error) = verify_artifact(&staging_dir, &staging) {
        let _ = std::fs::remove_dir_all(&staging_dir);
        return Err(error);
    }
    if let Err(error) = std::fs::rename(&staging_dir, &destination) {
        let _ = std::fs::remove_dir_all(&staging_dir);
        return Err(ArtifactError::VerificationMismatch(format!(
            "cannot commit `{}`: {error}",
            destination.display(),
        )));
    }
    Ok(destination)
}

/// Convenience: content identity of a text file.
#[must_use]
pub fn content_id_of_text(text: &str) -> ContentId {
    content_id_of_str(text)
}

// Artifact-class protocol tests moved to `tests/emath-artifact/tests/artifact_class.rs`.
