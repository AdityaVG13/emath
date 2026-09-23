use super::{Path, LanguageDistribution, LanguageImageError, collect_capsule_paths, read_text, capsule_documents, parse_feature_capsule, relative_source, LanguageSourceMapEntry, BTreeSet, MeaningSpine, inferred_class, MeaningEdge, MeaningResource, MeaningEdgeKind, FromStr, authority_from_capsules, FeatureAuthorityEntry, BTreeMap, LanguageImage, compile_reference_entries, REFERENCE_PARTITION, decode_reference_entries, first_reference_mismatch, fs, write_text, LANGUAGE_IMAGE_FILE, encode_image, LANGUAGE_LOCK_FILE, LANGUAGE_SOURCE_MAP_FILE};

pub fn compile_language_directory(root: &Path) -> Result<LanguageDistribution, LanguageImageError> {
    let spec = root.join("spec").join("constructors");
    if !spec.is_dir() {
        return Err(LanguageImageError::Io {
            path: spec,
            detail: "constructor image requires language/spec/constructors".to_string(),
        });
    }
    let mut paths = Vec::new();
    collect_capsule_paths(&spec, &mut paths)?;
    paths.sort();

    let mut capsules = Vec::new();
    let mut source_map = Vec::new();
    for path in paths {
        let text = read_text(&path)?;
        for document in capsule_documents(&text) {
            let (capsule, issues) = parse_feature_capsule(&document);
            if !issues.is_empty() {
                return Err(LanguageImageError::InvalidCapsule {
                    path: path.clone(),
                    issues: issues
                        .into_iter()
                        .map(|issue| format!("{}:{}:{}", issue.code, issue.line, issue.detail))
                        .collect(),
                });
            }
            let capsule = capsule.expect("issue-free capsule");
            let authored_source = relative_source(root, &path)?;
            source_map.push(LanguageSourceMapEntry {
                feature_id: capsule.feature_id.clone(),
                authored_source,
            });
            capsules.push(capsule);
        }
    }
    capsules.sort_by(|left, right| left.feature_id.cmp(&right.feature_id));
    for pair in capsules.windows(2) {
        if pair[0].feature_id == pair[1].feature_id {
            return Err(LanguageImageError::DuplicateFeature(
                pair[0].feature_id.clone(),
            ));
        }
    }

    let ids = capsules
        .iter()
        .map(|capsule| capsule.feature_id.clone())
        .collect::<BTreeSet<_>>();
    let unresolved = capsules
        .iter()
        .flat_map(|capsule| {
            capsule
                .edges
                .iter()
                .filter(|edge| !ids.contains(&edge.target))
                .map(|edge| edge.target.clone())
        })
        .collect::<BTreeSet<_>>();

    let mut spine = MeaningSpine::default();
    for capsule in &capsules {
        spine.register_feature(capsule.feature_id.clone(), capsule.class);
    }
    for dependency in unresolved {
        spine.register_feature(dependency.clone(), inferred_class(&dependency)?);
    }
    for capsule in &capsules {
        for edge in &capsule.edges {
            if edge.target == capsule.feature_id {
                continue;
            }
            spine
                .insert(MeaningEdge {
                    source: MeaningResource::Feature(capsule.feature_id.clone()),
                    kind: MeaningEdgeKind::from_str(&edge.kind).map_err(|error| {
                        LanguageImageError::OperationalContamination(format!("{error:?}"))
                    })?,
                    target: MeaningResource::Feature(edge.target.clone()),
                })
                .map_err(|error| {
                    LanguageImageError::OperationalContamination(format!("{error:?}"))
                })?;
        }
    }

    let authority = authority_from_capsules(&capsules)?;
    let authorities = authority
        .entries
        .iter()
        .map(|(feature_id, entry)| FeatureAuthorityEntry {
            feature_id: feature_id.clone(),
            state: entry.state.as_str().to_string(),
        })
        .collect::<Vec<_>>();
    let tables = crate::language_tables::generate_runtime_tables(&capsules)
        .map_err(|error| LanguageImageError::OperationalContamination(format!("{error:?}")))?;
    tables
        .verify()
        .map_err(|error| LanguageImageError::OperationalContamination(format!("{error:?}")))?;
    let authority_states = authority
        .entries
        .iter()
        .map(|(id, entry)| (id.to_string(), entry.state.as_str().to_string()))
        .collect::<BTreeMap<_, _>>();
    let views = crate::reference_views::generate_reference_views(&capsules, &authority_states)
        .map_err(|error| LanguageImageError::OperationalContamination(format!("{error:?}")))?;
    views
        .verify()
        .map_err(|error| LanguageImageError::OperationalContamination(format!("{error:?}")))?;
    let image = LanguageImage::build(
        &capsules,
        &spine,
        &BTreeMap::from([("runtime".to_string(), tables.bytes.clone())]),
        &authorities,
        &source_map,
        Vec::new(),
        None,
    )?;
    let compiled_references = compile_reference_entries(&capsules)?;
    let reference_page = image
        .load_partition(REFERENCE_PARTITION)
        .ok_or(LanguageImageError::InvalidSourceMap)?;
    let loaded_references = decode_reference_entries(reference_page)?;
    if let Some(feature) = first_reference_mismatch(&compiled_references, &loaded_references) {
        return Err(LanguageImageError::ReferenceBytecodeMismatch { feature });
    }
    let reference_programs = loaded_references
        .into_iter()
        .map(|(feature, entry)| (feature, entry.cell))
        .collect();
    let distribution = LanguageDistribution {
        capsules,
        spine,
        image,
        authority,
        runtime_tables: tables.bytes,
        reference_views: views.pages,
        reference_programs,
    };
    distribution.verify()?;
    Ok(distribution)
}

pub fn write_language_distribution(
    root: &Path,
    distribution: &LanguageDistribution,
) -> Result<(), LanguageImageError> {
    distribution.verify()?;
    let generated = root.join("generated");
    fs::create_dir_all(&generated).map_err(|error| LanguageImageError::Io {
        path: generated.clone(),
        detail: error.to_string(),
    })?;
    write_text(
        &root.join(LANGUAGE_IMAGE_FILE),
        &encode_image(&distribution.image),
    )?;
    write_text(
        &root.join(LANGUAGE_LOCK_FILE),
        &distribution.image.lock.canonical(),
    )?;
    write_text(
        &root.join(LANGUAGE_SOURCE_MAP_FILE),
        distribution
            .image
            .load_partition("language.sources")
            .ok_or(LanguageImageError::InvalidSourceMap)?,
    )?;
    write_text(
        &generated.join("runtime-tables.lock"),
        &distribution.runtime_tables,
    )?;
    for (name, page) in &distribution.reference_views {
        write_text(&generated.join(name), page)?;
    }
    Ok(())
}

pub fn load_language_distribution(root: &Path) -> Result<LanguageDistribution, LanguageImageError> {
    let distribution = compile_language_directory(root)?;
    let expected = [
        (
            root.join(LANGUAGE_IMAGE_FILE),
            encode_image(&distribution.image),
        ),
        (
            root.join(LANGUAGE_LOCK_FILE),
            distribution.image.lock.canonical(),
        ),
        (
            root.join(LANGUAGE_SOURCE_MAP_FILE),
            distribution
                .image
                .load_partition("language.sources")
                .ok_or(LanguageImageError::InvalidSourceMap)?
                .to_string(),
        ),
        (
            root.join("generated/runtime-tables.lock"),
            distribution.runtime_tables.clone(),
        ),
    ];
    for (path, contents) in expected {
        if read_text(&path)? != contents {
            return Err(LanguageImageError::GeneratedDrift { path });
        }
    }
    for (name, page) in &distribution.reference_views {
        let path = root.join("generated").join(name);
        if read_text(&path)? != *page {
            return Err(LanguageImageError::GeneratedDrift { path });
        }
    }
    Ok(distribution)
}

