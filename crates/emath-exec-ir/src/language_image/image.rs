use super::*;

impl LanguageImage {
    pub fn build(
        capsules: &[FeatureCapsule],
        spine: &MeaningSpine,
        tables: &BTreeMap<String, String>,
        authorities: &[FeatureAuthorityEntry],
        source_map: &[LanguageSourceMapEntry],
        prior_images: Vec<DistributionHash>,
        operational: Option<&[CanonicalField<'_>]>,
    ) -> Result<Self, LanguageImageError> {
        let mut ids = BTreeSet::new();
        for capsule in capsules {
            if !is_constructor_image_id(&capsule.feature_id) {
                return Err(LanguageImageError::NonConstructorIdentity(
                    capsule.feature_id.clone(),
                ));
            }
            if !ids.insert(capsule.feature_id.clone()) {
                return Err(LanguageImageError::DuplicateFeature(
                    capsule.feature_id.clone(),
                ));
            }
            if capsule.semantic_hash.as_str().starts_with("distribution-") {
                return Err(LanguageImageError::SemanticHashMismatch(
                    capsule.feature_id.clone(),
                ));
            }
        }
        for capsule in capsules {
            if !source_map
                .iter()
                .any(|entry| entry.feature_id == capsule.feature_id)
            {
                return Err(LanguageImageError::MissingSourceMap(
                    capsule.feature_id.clone(),
                ));
            }
        }

        let mut semantic_material = Vec::new();
        let mut sorted_capsules = capsules.iter().collect::<Vec<_>>();
        sorted_capsules.sort_by(|left, right| left.feature_id.cmp(&right.feature_id));
        for capsule in &sorted_capsules {
            semantic_material.extend_from_slice(&capsule.canonical_bytes());
        }
        semantic_material.extend_from_slice(spine.canonical().as_bytes());
        let semantic_hash = SemanticHash::new(&[CanonicalField::new(
            "language",
            &semantic_material,
        )
        .map_err(|error| LanguageImageError::OperationalContamination(error.to_string()))?])
        .map_err(|error| LanguageImageError::OperationalContamination(error.to_string()))?;

        let mut partitions = Vec::new();
        let capsules_body = sorted_capsules
            .iter()
            .map(|capsule| {
                format!(
                    "{} {} {} {}\n",
                    capsule.feature_id,
                    capsule.class,
                    capsule.maturity.as_str(),
                    capsule.semantic_hash
                )
            })
            .collect::<String>();
        partitions.push(ImagePartition::stamp(
            "language.capsules",
            PartitionKind::Cells,
            &capsules_body,
        ));
        partitions.push(ImagePartition::stamp(
            "language.spine",
            PartitionKind::Cells,
            &spine.canonical(),
        ));
        let table_body = if tables.is_empty() {
            "# empty\n".to_string()
        } else {
            tables
                .iter()
                .map(|(name, value)| format!("{name}={}:{value}\n", value.len()))
                .collect::<String>()
        };
        partitions.push(ImagePartition::stamp(
            "language.tables",
            PartitionKind::Bytecode,
            &table_body,
        ));
        let source_body = source_map.iter().collect::<Vec<_>>();
        let mut source_body = source_body;
        source_body.sort_by(|left, right| left.feature_id.cmp(&right.feature_id));
        let source_body = source_body
            .iter()
            .map(|entry| format!("{}={}\n", entry.feature_id, entry.authored_source))
            .collect::<String>();
        partitions.push(ImagePartition::stamp(
            "language.sources",
            PartitionKind::Docs,
            &source_body,
        ));
        let mut authorities = authorities.to_vec();
        authorities.sort_by(|left, right| left.feature_id.cmp(&right.feature_id));
        let authority_body = authorities
            .iter()
            .map(|entry| format!("{}={}\n", entry.feature_id, entry.state))
            .collect::<String>();
        partitions.push(ImagePartition::stamp(
            "language.authority",
            PartitionKind::Lock,
            &authority_body,
        ));
        let reference_entries = compile_reference_entries(capsules)?;
        partitions.push(ImagePartition::stamp(
            REFERENCE_PARTITION,
            PartitionKind::Bytecode,
            &encode_reference_partition(&reference_entries),
        ));
        partitions.sort_by(|left, right| left.name.cmp(&right.name));

        let mut distribution_material = String::new();
        distribution_material.push_str(LANGUAGE_IMAGE_SCHEMA);
        distribution_material.push('\n');
        distribution_material.push_str(semantic_hash.as_str());
        distribution_material.push('\n');
        for partition in &partitions {
            distribution_material.push_str(&partition.name);
            distribution_material.push('=');
            distribution_material.push_str(&partition.content_id);
            distribution_material.push('\n');
        }
        let distribution_hash = DistributionHash::new(&[CanonicalField::new(
            "image",
            distribution_material.as_bytes(),
        )
        .map_err(|error| LanguageImageError::OperationalContamination(error.to_string()))?])
        .map_err(|error| LanguageImageError::OperationalContamination(error.to_string()))?;
        let lock = LanguageImageLock {
            schema: LANGUAGE_LOCK_SCHEMA.to_string(),
            semantic_hash: semantic_hash.clone(),
            distribution_hash: distribution_hash.clone(),
            prior_images,
        };
        partitions.push(ImagePartition::stamp(
            "language.lock",
            PartitionKind::Lock,
            &lock.canonical(),
        ));
        partitions.sort_by(|left, right| left.name.cmp(&right.name));
        let image = SemanticImage {
            pack_name: "language".to_string(),
            partitions,
            image_id: distribution_hash.to_string(),
        };
        let operational_hash = operational
            .map(OperationalHash::new)
            .transpose()
            .map_err(|error| LanguageImageError::OperationalContamination(error.to_string()))?;
        Ok(Self {
            schema: LANGUAGE_IMAGE_SCHEMA.to_string(),
            semantic_hash,
            distribution_hash,
            operational_hash,
            image,
            lock,
        })
    }

    pub fn verify(&self) -> Result<(), LanguageImageError> {
        if self.schema != LANGUAGE_IMAGE_SCHEMA {
            return Err(LanguageImageError::UnknownSchema(self.schema.clone()));
        }
        self.image
            .validate_partitions()
            .map_err(LanguageImageError::CorruptImage)?;
        if self.lock.schema != LANGUAGE_LOCK_SCHEMA
            || self.lock.semantic_hash != self.semantic_hash
            || self.lock.distribution_hash != self.distribution_hash
            || self.image.image_id != self.distribution_hash.to_string()
        {
            return Err(LanguageImageError::StaleLock);
        }
        Ok(())
    }

    #[must_use]
    pub fn load_partition(&self, name: &str) -> Option<&str> {
        self.image.load(name)
    }

    #[must_use]
    pub fn authored_source(&self, feature: &FeatureId) -> Option<&str> {
        self.load_partition("language.sources")?
            .lines()
            .find_map(|line| {
                let (id, source) = line.split_once('=')?;
                (id == feature.as_str()).then_some(source)
            })
    }

    pub fn verify_hash_text(hash: &str) -> Result<(), LanguageImageError> {
        DistributionHash::from_str(hash)
            .map(|_| ())
            .map_err(|_| LanguageImageError::UnknownSchema(hash.to_string()))
    }

    /// Loads the `language.reference` partition back as validated generic
    /// programs: every entry is recompiled from its canonical term and the
    /// embedded bytecode must reproduce byte-for-byte, so tampered or stale
    /// pages refuse typed instead of loading partial authority.
    pub fn decode_reference_partition(
        page: &str,
    ) -> Result<BTreeMap<FeatureId, CompiledCell>, LanguageImageError> {
        decode_reference_entries(page).map(|entries| {
            entries
                .into_iter()
                .map(|(feature, entry)| (feature, entry.cell))
                .collect()
        })
    }
}

