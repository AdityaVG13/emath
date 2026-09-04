//! Typed deserialization of manifests and records from JSON.

use super::*;

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
