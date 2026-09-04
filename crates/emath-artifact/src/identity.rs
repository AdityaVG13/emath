//! Content identity and required-path computation.

use super::*;

/// The four metadata documents every artifact package carries regardless
/// of class: the durable manifest, source map, resolution plan and
/// evidence bundle.
pub(super) const METADATA_PATHS: [&str; 4] = [
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

pub(super) fn quote(s: &str) -> String {
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
