use super::*;

pub(super) fn authority_from_capsules(
    capsules: &[FeatureCapsule],
) -> Result<AuthorityLock, LanguageImageError> {
    let mut lock = AuthorityLock::default();
    for capsule in capsules {
        let state = capsule
            .slots
            .get("authority_target")
            .and_then(slot_value)
            .ok_or_else(|| LanguageImageError::MissingAuthority(capsule.feature_id.clone()))?;
        let state =
            AuthorityState::from_str(state).map_err(|_| LanguageImageError::InvalidAuthority {
                feature: capsule.feature_id.clone(),
                state: state.to_string(),
            })?;
        if state == AuthorityState::CapsuleActive && capsule.has_blocking_hole() {
            return Err(LanguageImageError::BlockingHole(capsule.feature_id.clone()));
        }
        if lock
            .entries
            .insert(
                capsule.feature_id.clone(),
                AuthorityEntry {
                    state,
                    active_source: match state {
                        AuthorityState::CapsuleActive | AuthorityState::CapsuleCandidate => {
                            "capsule".to_string()
                        }
                        AuthorityState::Retired => "none".to_string(),
                        _ => "legacy".to_string(),
                    },
                    semantic_hash: capsule.semantic_hash.clone(),
                },
            )
            .is_some()
        {
            return Err(LanguageImageError::DuplicateAuthority(
                capsule.feature_id.clone(),
            ));
        }
    }
    Ok(lock)
}

pub(super) fn slot_value(slot: &emath_ir::CapsuleSlot) -> Option<&str> {
    match slot {
        emath_ir::CapsuleSlot::Value(value) => Some(value),
        _ => None,
    }
}

pub(super) fn authority_page_text(authority: &AuthorityLock) -> String {
    authority
        .entries
        .iter()
        .map(|(id, entry)| format!("{id}={}\n", entry.state.as_str()))
        .collect()
}


pub(super) fn collect_capsule_paths(root: &Path, output: &mut Vec<PathBuf>) -> Result<(), LanguageImageError> {
    let entries = fs::read_dir(root).map_err(|error| LanguageImageError::Io {
        path: root.to_path_buf(),
        detail: error.to_string(),
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| LanguageImageError::Io {
            path: root.to_path_buf(),
            detail: error.to_string(),
        })?;
        let path = entry.path();
        if path.is_dir() {
            collect_capsule_paths(&path, output)?;
        } else if path.extension().and_then(|value| value.to_str()) == Some("emath") {
            output.push(path);
        }
    }
    Ok(())
}

pub(super) fn relative_source(root: &Path, path: &Path) -> Result<String, LanguageImageError> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| LanguageImageError::InvalidSourceMap)?;
    Ok(Path::new("language")
        .join(relative)
        .to_string_lossy()
        .replace('\\', "/"))
}

pub(super) fn read_text(path: &Path) -> Result<String, LanguageImageError> {
    fs::read_to_string(path).map_err(|error| LanguageImageError::Io {
        path: path.to_path_buf(),
        detail: error.to_string(),
    })
}

pub(super) fn write_text(path: &Path, contents: &str) -> Result<(), LanguageImageError> {
    fs::write(path, contents).map_err(|error| LanguageImageError::Io {
        path: path.to_path_buf(),
        detail: error.to_string(),
    })
}

pub(super) fn encode_image(image: &LanguageImage) -> String {
    let mut output = format!(
        "schema={}\nsemantic_hash={}\ndistribution_hash={}\n",
        image.schema, image.semantic_hash, image.distribution_hash
    );
    for partition in &image.image.partitions {
        output.push_str(&format!(
            "partition {} {} {} {}\n{}",
            partition.name,
            partition.kind.as_str(),
            partition.content_id,
            partition.body.len(),
            partition.body
        ));
        if !partition.body.ends_with('\n') {
            output.push('\n');
        }
    }
    output
}

pub(super) fn inferred_class(feature: &FeatureId) -> Result<emath_ir::FeatureClass, LanguageImageError> {
    let class = match feature.class() {
        "constitution" => emath_ir::FeatureClass::Constitution,
        "syntax" => emath_ir::FeatureClass::Syntax,
        "kind" => emath_ir::FeatureClass::Kind,
        "section" => emath_ir::FeatureClass::Section,
        "surface" => emath_ir::FeatureClass::Surface,
        "symbol" => emath_ir::FeatureClass::Symbol,
        "type" => emath_ir::FeatureClass::Type,
        "binder" => emath_ir::FeatureClass::Binder,
        "capability" => emath_ir::FeatureClass::Capability,
        "theory" => emath_ir::FeatureClass::Theory,
        "instance" => emath_ir::FeatureClass::Instance,
        "goal" => emath_ir::FeatureClass::Goal,
        "method" => emath_ir::FeatureClass::Method,
        "world" => emath_ir::FeatureClass::World,
        "provider" => emath_ir::FeatureClass::Provider,
        "effect" => emath_ir::FeatureClass::Effect,
        "artifact" => emath_ir::FeatureClass::Artifact,
        "diagnostic" => emath_ir::FeatureClass::Diagnostic,
        "migration" => emath_ir::FeatureClass::Migration,
        "field_pack" => emath_ir::FeatureClass::FieldPack,
        _ => {
            return Err(LanguageImageError::UnresolvedDependency {
                feature: feature.clone(),
                dependency: feature.clone(),
            });
        }
    };
    Ok(class)
}

