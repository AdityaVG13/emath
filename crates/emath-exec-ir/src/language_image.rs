//! Deterministic Language Image layered on the existing partition model.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use emath_core::{
    CanonicalField, DistributionHash, FeatureId, OperationalHash, SemanticHash, Span,
};
use emath_ir::{
    AuthorityEntry, AuthorityLock, AuthorityState, FeatureCapsule, MeaningEdge, MeaningEdgeKind,
    MeaningResource, MeaningSpine,
};
use emath_schema::{capsule_documents, parse_feature_capsule};
use emath_term::{Signature, SymbolId, Term, TermError};

use crate::term_compile::{CompiledCell, ParamShape};
use crate::{DomainObligation, EmirOp, EmirProgram, EmirValue, optimize};

use crate::image::{ImagePartition, ImageRefusal, PartitionKind, SemanticImage};

pub const LANGUAGE_IMAGE_SCHEMA: &str = "emath.language-image";
pub const LANGUAGE_LOCK_SCHEMA: &str = "emath.language-lock";
pub const LANGUAGE_SOURCE_MAP_SCHEMA: &str = "emath.language-source-map";
pub const LANGUAGE_IMAGE_FILE: &str = "generated/language.image";
pub const LANGUAGE_LOCK_FILE: &str = "language.lock";
pub const LANGUAGE_SOURCE_MAP_FILE: &str = "generated/source-map.lock";

/// The standard image is these constructor/carrier identities only.
pub const CONSTRUCTOR_IMAGE_IDS: &[&str] = &[
    "std.capability.scalar",
    "std.kind.function",
    "std.kind.object",
    "std.kind.query",
    "std.syntax.quote",
    "std.syntax.recur",
    "std.world.reference",
];

#[must_use]
pub fn is_constructor_image_id(id: &FeatureId) -> bool {
    CONSTRUCTOR_IMAGE_IDS.contains(&id.as_str())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeatureAuthorityEntry {
    pub feature_id: FeatureId,
    pub state: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LanguageSourceMapEntry {
    pub feature_id: FeatureId,
    pub authored_source: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LanguageImageLock {
    pub schema: String,
    pub semantic_hash: SemanticHash,
    pub distribution_hash: DistributionHash,
    pub prior_images: Vec<DistributionHash>,
}

impl LanguageImageLock {
    #[must_use]
    pub fn canonical(&self) -> String {
        let mut prior = self.prior_images.clone();
        prior.sort();
        format!(
            "schema={}\nsemantic_hash={}\ndistribution_hash={}\nprior_images={}\n",
            self.schema,
            self.semantic_hash,
            self.distribution_hash,
            prior
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(",")
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LanguageImage {
    pub schema: String,
    pub semantic_hash: SemanticHash,
    pub distribution_hash: DistributionHash,
    pub operational_hash: Option<OperationalHash>,
    pub image: SemanticImage,
    pub lock: LanguageImageLock,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LanguageImageError {
    DuplicateFeature(FeatureId),
    MissingSourceMap(FeatureId),
    OperationalContamination(String),
    SemanticHashMismatch(FeatureId),
    StaleLock,
    CorruptImage(ImageRefusal),
    UnknownSchema(String),
    Io {
        path: PathBuf,
        detail: String,
    },
    InvalidCapsule {
        path: PathBuf,
        issues: Vec<String>,
    },
    DuplicateAuthority(FeatureId),
    MissingAuthority(FeatureId),
    InvalidAuthority {
        feature: FeatureId,
        state: String,
    },
    UnresolvedDependency {
        feature: FeatureId,
        dependency: FeatureId,
    },
    BlockingHole(FeatureId),
    InvalidSourceMap,
    InvalidReferenceBody {
        feature: FeatureId,
        detail: String,
    },
    ReferencePartitionMalformed(String),
    ReferenceBytecodeMismatch {
        feature: FeatureId,
    },
    GeneratedDrift {
        path: PathBuf,
    },
    NonConstructorIdentity(FeatureId),
}

/// `PartialEq` is honest field-wise equality; `Eq` is deliberately
/// absent because reference programs carry f64 bytecode payloads.
#[derive(Clone, Debug, PartialEq)]
pub struct LanguageDistribution {
    pub capsules: Vec<FeatureCapsule>,
    pub spine: MeaningSpine,
    pub image: LanguageImage,
    pub authority: AuthorityLock,
    pub runtime_tables: String,
    pub reference_views: BTreeMap<String, String>,
    pub reference_programs: BTreeMap<FeatureId, CompiledCell>,
}

impl LanguageDistribution {
    #[must_use]
    pub fn authority_map(&self) -> BTreeMap<FeatureId, String> {
        self.authority
            .entries
            .iter()
            .map(|(id, entry)| (id.clone(), entry.state.as_str().to_string()))
            .collect()
    }

    /// Scoped authority rollback: demotes one capsule-active feature to
    /// `rollback-pending` and reseals the whole distribution. The image,
    /// lock, authority partition, and reference views are rebuilt through
    /// the same `LanguageImage::build` path as compile, with the superseded
    /// distribution hash recorded in `prior_images`, so the result verifies
    /// instead of tripping `StaleLock` at install. Feature identity and
    /// capsule bytes are untouched; only the authority row changes state.
    pub fn rollback_feature(&self, feature: &FeatureId) -> Result<Self, LanguageImageError> {
        self.verify()?;
        let Some(entry) = self.authority.entries.get(feature) else {
            return Err(LanguageImageError::MissingAuthority(feature.clone()));
        };
        if entry.state != AuthorityState::CapsuleActive {
            return Err(LanguageImageError::InvalidAuthority {
                feature: feature.clone(),
                state: entry.state.as_str().to_string(),
            });
        }
        let mut authority = self.authority.clone();
        let entry = authority.entries.get_mut(feature).expect("checked above");
        entry.state = AuthorityState::RollbackPending;
        entry.active_source = "legacy".to_string();

        let authorities = authority
            .entries
            .iter()
            .map(|(feature_id, entry)| FeatureAuthorityEntry {
                feature_id: feature_id.clone(),
                state: entry.state.as_str().to_string(),
            })
            .collect::<Vec<_>>();
        let source_map = self
            .image
            .load_partition("language.sources")
            .ok_or(LanguageImageError::InvalidSourceMap)?
            .lines()
            .filter_map(|line| {
                let (id, source) = line.split_once('=')?;
                Some(LanguageSourceMapEntry {
                    feature_id: FeatureId::from_str(id).ok()?,
                    authored_source: source.to_string(),
                })
            })
            .collect::<Vec<_>>();
        let tables = BTreeMap::from([("runtime".to_string(), self.runtime_tables.clone())]);
        let mut prior_images = self.image.lock.prior_images.clone();
        prior_images.push(self.image.distribution_hash.clone());
        let image = LanguageImage::build(
            &self.capsules,
            &self.spine,
            &tables,
            &authorities,
            &source_map,
            prior_images,
            None,
        )?;
        let reference_page = image
            .load_partition(REFERENCE_PARTITION)
            .ok_or(LanguageImageError::InvalidSourceMap)?;
        let reference_programs = decode_reference_entries(reference_page)?
            .into_iter()
            .map(|(feature, entry)| (feature, entry.cell))
            .collect();
        let authority_states = authority
            .entries
            .iter()
            .map(|(id, entry)| (id.to_string(), entry.state.as_str().to_string()))
            .collect::<BTreeMap<_, _>>();
        let views =
            crate::reference_views::generate_reference_views(&self.capsules, &authority_states)
                .map_err(|error| {
                    LanguageImageError::OperationalContamination(format!("{error:?}"))
                })?;
        views
            .verify()
            .map_err(|error| LanguageImageError::OperationalContamination(format!("{error:?}")))?;
        let distribution = Self {
            capsules: self.capsules.clone(),
            spine: self.spine.clone(),
            image,
            authority,
            runtime_tables: self.runtime_tables.clone(),
            reference_views: views.pages,
            reference_programs,
        };
        distribution.verify()?;
        Ok(distribution)
    }

    pub fn verify(&self) -> Result<(), LanguageImageError> {
        self.image.verify()?;
        let source_page = self
            .image
            .load_partition("language.sources")
            .ok_or(LanguageImageError::InvalidSourceMap)?;
        if source_page.is_empty()
            || self
                .capsules
                .iter()
                .any(|capsule| self.image.authored_source(&capsule.feature_id).is_none())
        {
            return Err(LanguageImageError::InvalidSourceMap);
        }
        let authority_page = self
            .image
            .load_partition("language.authority")
            .ok_or(LanguageImageError::StaleLock)?;
        if authority_page != authority_page_text(&self.authority) {
            return Err(LanguageImageError::StaleLock);
        }
        for capsule in &self.capsules {
            let Some(entry) = self.authority.entries.get(&capsule.feature_id) else {
                return Err(LanguageImageError::MissingAuthority(
                    capsule.feature_id.clone(),
                ));
            };
            if entry.semantic_hash != capsule.semantic_hash {
                return Err(LanguageImageError::SemanticHashMismatch(
                    capsule.feature_id.clone(),
                ));
            }
            if entry.state == AuthorityState::CapsuleActive && capsule.has_blocking_hole() {
                return Err(LanguageImageError::BlockingHole(capsule.feature_id.clone()));
            }
        }
        // The public reference map is never trusted: it must still equal
        // the decoded `language.reference` partition, so a map mutated
        // after compile refuses instead of installing forged authority.
        let reference_page = self
            .image
            .load_partition(REFERENCE_PARTITION)
            .ok_or(LanguageImageError::InvalidSourceMap)?;
        let decoded = decode_reference_entries(reference_page)?;
        if let Some(feature) = first_installed_map_mismatch(&self.reference_programs, &decoded) {
            return Err(LanguageImageError::ReferenceBytecodeMismatch { feature });
        }
        Ok(())
    }
}

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

fn authority_from_capsules(
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

fn slot_value(slot: &emath_ir::CapsuleSlot) -> Option<&str> {
    match slot {
        emath_ir::CapsuleSlot::Value(value) => Some(value),
        _ => None,
    }
}

fn authority_page_text(authority: &AuthorityLock) -> String {
    authority
        .entries
        .iter()
        .map(|(id, entry)| format!("{id}={}\n", entry.state.as_str()))
        .collect()
}


fn collect_capsule_paths(root: &Path, output: &mut Vec<PathBuf>) -> Result<(), LanguageImageError> {
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

fn relative_source(root: &Path, path: &Path) -> Result<String, LanguageImageError> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| LanguageImageError::InvalidSourceMap)?;
    Ok(Path::new("language")
        .join(relative)
        .to_string_lossy()
        .replace('\\', "/"))
}

fn read_text(path: &Path) -> Result<String, LanguageImageError> {
    fs::read_to_string(path).map_err(|error| LanguageImageError::Io {
        path: path.to_path_buf(),
        detail: error.to_string(),
    })
}

fn write_text(path: &Path, contents: &str) -> Result<(), LanguageImageError> {
    fs::write(path, contents).map_err(|error| LanguageImageError::Io {
        path: path.to_path_buf(),
        detail: error.to_string(),
    })
}

fn encode_image(image: &LanguageImage) -> String {
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

fn inferred_class(feature: &FeatureId) -> Result<emath_ir::FeatureClass, LanguageImageError> {
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

/// Image partition carrying the compiled reference programs. The page is
/// DATA (canonical text), never generated Rust source, and it is the
/// loaded-reference source of truth for consumers.
const REFERENCE_PARTITION: &str = "language.reference";
const REFERENCE_MODE_SLOT: &str = "reference";
const REFERENCE_MODE_AUTHORED: &str = "authored";
const REFERENCE_PARAMS_SLOT: &str = "reference_params";
const REFERENCE_SIGNATURE_SLOT: &str = "reference_signature";
const REFERENCE_BODY_SLOT: &str = "reference_body";
const REFERENCE_NONE_PAGE: &str = "# none\n";

/// One derived reference program: the authored canonical term it was
/// compiled from, the declared parameters with shapes, and the compiled
/// cell. Kept together so encode/decode/recompile stay one source of truth.
#[derive(Clone, Debug)]
struct ReferenceEntry {
    term: Term,
    defaults: Vec<Term>,
    params: Vec<(String, ParamShape)>,
    cell: CompiledCell,
}

/// The closed machine-neutral scalar reference vocabulary: symbol tokens
/// mapped onto existing generic EMIR operations. Tokens name operators,
/// never features; extending the set is a schema-recorded decision, not a
/// feature-name branch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReferenceOperator {
    Add,
    Sub,
    Mul,
    Div,
    Neg,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
    And,
    Or,
    Not,
    IsFinite,
    #[allow(dead_code)]
    Unary(crate::BuiltinId),
    #[allow(dead_code)]
    Binary(crate::BuiltinId),
    #[allow(dead_code)]
    ToInt,
    #[allow(dead_code)]
    IntegerQuotient,
    SameBits,
    #[allow(dead_code)]
    MatrixRows,
    #[allow(dead_code)]
    MatrixCols,
    #[allow(dead_code)]
    MatrixAt,
    #[allow(dead_code)]
    MatrixPack,
    #[allow(dead_code)]
    TensorShape,
    #[allow(dead_code)]
    TensorPack,
    #[allow(dead_code)]
    DenseIndex,
    #[allow(dead_code)]
    Infinity,
    #[allow(dead_code)]
    Exp2,
    #[allow(dead_code)]
    PowF,
    #[allow(dead_code)]
    PowI,
    TextTrim,
    TextLength,
    IndexText,
    TextByte,
    FormatScientific,
    ParseF64,
    SortTotal,
}

impl ReferenceOperator {
    fn resolve(symbol: &str, arity: usize) -> Option<Self> {
        match (symbol, arity) {
            ("add", 2) => Some(Self::Add),
            ("sub", 2) => Some(Self::Sub),
            ("mul", 2) => Some(Self::Mul),
            ("div", 2) => Some(Self::Div),
            ("neg", 1) => Some(Self::Neg),
            ("lt", 2) => Some(Self::Lt),
            ("le", 2) => Some(Self::Le),
            ("gt", 2) => Some(Self::Gt),
            ("ge", 2) => Some(Self::Ge),
            ("eq", 2) => Some(Self::Eq),
            ("ne", 2) => Some(Self::Ne),
            ("and", 2) => Some(Self::And),
            ("or", 2) => Some(Self::Or),
            ("not", 1) => Some(Self::Not),
            ("is_finite", 1) => Some(Self::IsFinite),
            ("text_trim", 1) => Some(Self::TextTrim),
            ("text_length", 1) => Some(Self::TextLength),
            ("index_text", 1) => Some(Self::IndexText),
            ("text_byte", 2) => Some(Self::TextByte),
            ("format_scientific", 2) => Some(Self::FormatScientific),
            ("parse_f64", 1) => Some(Self::ParseF64),
            ("sort_total", 1) => Some(Self::SortTotal),
            ("same_bits", 2) => Some(Self::SameBits),
            _ => None,
        }
    }

    fn lower(self, operands: &[EmirValue], obligations: &mut Vec<DomainObligation>) -> EmirOp {
        match self {
            Self::Add => EmirOp::F64Add(operands[0], operands[1]),
            Self::Sub => EmirOp::F64Sub(operands[0], operands[1]),
            Self::Mul => EmirOp::F64Mul(operands[0], operands[1]),
            Self::Div => {
                obligations.push(DomainObligation::DivisionNonZero);
                EmirOp::F64Div(operands[0], operands[1])
            }
            Self::Neg => EmirOp::Neg(operands[0]),
            Self::Lt => EmirOp::Lt(operands[0], operands[1]),
            Self::Le => EmirOp::Le(operands[0], operands[1]),
            Self::Gt => EmirOp::Gt(operands[0], operands[1]),
            Self::Ge => EmirOp::Ge(operands[0], operands[1]),
            Self::Eq => EmirOp::Eq(operands[0], operands[1]),
            Self::Ne => EmirOp::Ne(operands[0], operands[1]),
            Self::And => EmirOp::And(operands[0], operands[1]),
            Self::Or => EmirOp::Or(operands[0], operands[1]),
            Self::Not => EmirOp::Not(operands[0]),
            Self::IsFinite => EmirOp::IsFinite(operands[0]),
            Self::Unary(builtin) => EmirOp::UnaryBuiltin(builtin, operands[0]),
            Self::Binary(builtin) => EmirOp::BinaryBuiltin(builtin, operands[0], operands[1]),
            Self::ToInt => EmirOp::ToInt(operands[0]),
            Self::IntegerQuotient => EmirOp::IntegerQuotient(operands[0], operands[1]),
            Self::SameBits => EmirOp::SameBits(operands[0], operands[1]),
            Self::MatrixRows => EmirOp::MatrixRows(operands[0]),
            Self::MatrixCols => EmirOp::MatrixCols(operands[0]),
            Self::MatrixAt => EmirOp::MatrixIndex { matrix: operands[0], row: operands[1], col: operands[2] },
            Self::MatrixPack => EmirOp::MatrixPack { rows: operands[0], cols: operands[1], data: operands[2] },
            Self::Infinity => EmirOp::ConstF64(f64::INFINITY.to_bits()),
            Self::Exp2 => EmirOp::F64Exp2(operands[0]),
            Self::PowF => EmirOp::F64Pow(operands[0], operands[1]),
            Self::PowI => EmirOp::F64PowI(operands[0], operands[1]),
            Self::TextTrim => EmirOp::TextTrim(operands[0]),
            Self::TextLength => EmirOp::TextLength(operands[0]),
            Self::IndexText => EmirOp::IndexText(operands[0]),
            Self::TextByte => EmirOp::TextByte(operands[0], operands[1]),
            Self::FormatScientific => EmirOp::FormatScientific(operands[0], operands[1]),
            Self::ParseF64 => EmirOp::ParseF64(operands[0]),
            Self::SortTotal => EmirOp::F64SortTotal(operands[0]),
            Self::TensorShape => EmirOp::TensorShape(operands[0]),
            Self::TensorPack => EmirOp::TensorPack { shape: operands[0], data: operands[1] },
            Self::DenseIndex => EmirOp::DenseIndex { dense: operands[0], index: operands[1] },
        }
    }
}

/// Compiles the authored reference bodies of a capsule set into
/// capability-keyed entries, in deterministic feature order.
fn compile_reference_entries(
    capsules: &[FeatureCapsule],
) -> Result<BTreeMap<FeatureId, ReferenceEntry>, LanguageImageError> {
    let mut entries = BTreeMap::new();
    for capsule in capsules {
        if let Some(entry) = reference_entry_for_capsule(capsule)? {
            if entries.insert(capsule.feature_id.clone(), entry).is_some() {
                return Err(LanguageImageError::DuplicateFeature(
                    capsule.feature_id.clone(),
                ));
            }
        }
    }
    let mut active = BTreeSet::new();
    let mut complete = BTreeSet::new();
    for feature in entries.keys() {
        validate_reference_calls(feature, &entries, &mut active, &mut complete)?;
    }
    Ok(entries)
}

fn validate_reference_calls(
    feature: &FeatureId,
    entries: &BTreeMap<FeatureId, ReferenceEntry>,
    active: &mut BTreeSet<FeatureId>,
    complete: &mut BTreeSet<FeatureId>,
) -> Result<(), LanguageImageError> {
    if complete.contains(feature) { return Ok(()); }
    if !active.insert(feature.clone()) {
        return Err(LanguageImageError::InvalidReferenceBody { feature: feature.clone(), detail: "cyclic authored calls require explicit bounded iteration".into() });
    }
    fn dependencies(program: &EmirProgram, out: &mut BTreeSet<FeatureId>) {
        for (op, _) in &program.ops {
            match op {
                EmirOp::ApplyCapability { capability, .. } => { if let Ok(feature) = FeatureId::from_str(capability) { out.insert(feature); } }
                EmirOp::Branch { then_body, else_body, .. } => { dependencies(then_body, out); dependencies(else_body, out); }
                EmirOp::Iterate { body, stop, .. } => { if let Some(stop) = stop { dependencies(stop, out); } dependencies(body, out); }
                EmirOp::Fold { body, .. } | EmirOp::Collect { body, .. }
            | EmirOp::CallFrame { body, .. } | EmirOp::ProgramLiteral { body, .. } => dependencies(body, out),
                _ => {}
            }
        }
    }
    if let Some(entry) = entries.get(feature) {
        let mut called = BTreeSet::new();
        dependencies(&entry.cell.program, &mut called);
        for default in &entry.cell.defaults { dependencies(default, &mut called); }
        for callee in called {
            if entries.contains_key(&callee) { validate_reference_calls(&callee, entries, active, complete)?; }
        }
    }
    active.remove(feature);
    complete.insert(feature.clone());
    Ok(())
}

/// Derives one capsule's reference entry, or `None` when the capsule
/// declares no executable reference body. All three body slots are
/// required together, and their presence requires the `authored` mode.
fn reference_entry_for_capsule(
    capsule: &FeatureCapsule,
) -> Result<Option<ReferenceEntry>, LanguageImageError> {
    let params_slot = reference_slot(capsule, REFERENCE_PARAMS_SLOT);
    let signature_slot = reference_slot(capsule, REFERENCE_SIGNATURE_SLOT);
    let body_slot = reference_slot(capsule, REFERENCE_BODY_SLOT);
    let defaults_slot = reference_slot(capsule, "reference_defaults");
    if params_slot.is_none()
        && signature_slot.is_none()
        && body_slot.is_none()
        && defaults_slot.is_none()
    {
        return Ok(None);
    }
    let refuse = |detail: String| {
        Err(LanguageImageError::InvalidReferenceBody {
            feature: capsule.feature_id.clone(),
            detail,
        })
    };
    let (Some(params_text), Some(body_text)) = (params_slot, body_slot) else {
        return refuse(
            "executable reference bodies carry reference_params and reference_body"
                .to_string(),
        );
    };
    match reference_slot(capsule, REFERENCE_MODE_SLOT) {
        Some(REFERENCE_MODE_AUTHORED) => {}
        other => {
            return refuse(format!(
                "an executable reference body requires reference mode \
                 `{REFERENCE_MODE_AUTHORED}`, found `{}`",
                other.unwrap_or("missing"),
            ));
        }
    }
    let mut params = Vec::new();
    for token in params_text.split(',') {
        let name = token.trim();
        if name.is_empty() || params.iter().any(|(declared, _)| declared == name) {
            return refuse(format!(
                "reference_params `{params_text}` names no parameter or repeats one"
            ));
        }
        params.push((name.to_string(), ParamShape::Scalar));
    }
    let term = Term::parse_body(body_text).map_err(|error| {
        LanguageImageError::InvalidReferenceBody {
            feature: capsule.feature_id.clone(),
            detail: format!("reference_body is not an emath-term: {error:?}"),
        }
    })?;
    let signature = match signature_slot {
        Some(signature_text) => {
            let mut signature = Signature::default();
            for pair in signature_text.split(',') {
                let Some((symbol, arity)) = pair.trim().rsplit_once('=') else {
                    return refuse(format!(
                        "reference_signature `{signature_text}` must declare `symbol=arity` pairs"
                    ));
                };
                let Ok(arity) = arity.trim().parse::<usize>() else {
                    return refuse(format!(
                        "reference_signature `{signature_text}` declares a non-numeric arity"
                    ));
                };
                if let Err(error) = signature.insert(SymbolId(symbol.trim().to_string()), arity) {
                    return refuse(format!(
                        "reference_signature `{signature_text}` conflicts: {error:?}"
                    ));
                }
            }
            signature
        }
        None => term.inferred_signature().map_err(|error| {
            LanguageImageError::InvalidReferenceBody {
                feature: capsule.feature_id.clone(),
                detail: format!("could not infer reference signature: {error:?}"),
            }
        })?,
    };
    let Some(semantics) = reference_slot(capsule, "semantics") else {
        return refuse("an executable reference body requires the semantics slot".to_string());
    };
    let shapes = declared_input_shapes(semantics);
    if shapes.len() != params.len() {
        return refuse(format!(
            "semantics declares {} input(s) but the reference body names {} parameter(s)",
            shapes.len(),
            params.len(),
        ));
    }
    for ((_, shape), declared) in params.iter_mut().zip(shapes) {
        *shape = declared;
    }
    let defaults = defaults_slot
        .map(|text| {
            let mut depth = 0usize;
            let mut escaped = false;
            text.split(|ch| {
                if escaped {
                    escaped = false;
                    return false;
                }
                match ch {
                    '\\' => escaped = true,
                    '(' => depth += 1,
                    ')' => depth = depth.saturating_sub(1),
                    ';' if depth == 0 => return true,
                    _ => {}
                }
                false
            })
            .map(|term| Term::parse_canonical(term.trim()))
            .collect::<Result<Vec<_>, _>>()
        })
        .transpose()
        .map_err(|error| LanguageImageError::InvalidReferenceBody {
            feature: capsule.feature_id.clone(),
            detail: format!("invalid reference default: {error:?}"),
        })?
        .unwrap_or_default();
    if defaults_slot.is_some() {
        let arity = semantics
            .split(';')
            .find_map(|part| part.trim().strip_prefix("arity="))
            .and_then(crate::native_kernel::parse_kernel_arity);
        let range = match arity {
            Some(crate::native_kernel::KernelArity::Exact(count)) => Some((count, count)),
            Some(crate::native_kernel::KernelArity::Bounded { min, max }) => Some((min, max)),
            None => None,
        };
        if params
            .len()
            .checked_sub(defaults.len())
            .map(|min| (min, params.len()))
            != range
        {
            return refuse("reference defaults do not match the declared arity range".into());
        }
    }
    compile_reference_with_defaults(
        &term,
        &defaults,
        &signature,
        &params,
        capsule.feature_id.as_str(),
    )
    .map(|cell| {
        Some(ReferenceEntry {
            term,
            defaults,
            params,
            cell,
        })
    })
    .map_err(|detail| LanguageImageError::InvalidReferenceBody {
        feature: capsule.feature_id.clone(),
        detail,
    })
}

fn reference_slot<'a>(capsule: &'a FeatureCapsule, name: &str) -> Option<&'a str> {
    match capsule.slots.get(name) {
        Some(emath_ir::CapsuleSlot::Value(value)) => Some(value.as_str()),
        _ => None,
    }
}

/// Shapes declared by the semantics slot's `inputs=` field, in argument
/// order. The mapping is generic over the declared type tokens —
/// `Vector<Float64>` and `Matrix<Float64>` carry their carriers, every
/// other token is a scalar.
fn declared_input_shapes(semantics: &str) -> Vec<ParamShape> {
    semantics
        .split(';')
        .find_map(|field| field.trim().strip_prefix("inputs="))
        .map(|inputs| {
            inputs
                .split(',')
                .map(|token| {
                    let token = token.trim();
                    if token == "Rat" {
                        ParamShape::Rational
                    } else if token.starts_with("Vector") {
                        ParamShape::Vector
                    } else if token.starts_with("Matrix") {
                        ParamShape::Matrix
                    } else {
                        ParamShape::Scalar
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Compiles one validated reference term into a `CompiledCell` over the
/// closed scalar vocabulary. The refusal detail is caller-shaped: capsule
/// derivation and image recompilation wrap it in their own error types.
fn compile_reference_term(
    term: &Term,
    signature: &Signature,
    params: &[(String, ParamShape)],
    capability: &str,
) -> Result<CompiledCell, String> {
    signature.validate(term).map_err(|error| match error {
        TermError::UnknownSymbol(symbol) => {
            format!("reference term uses undeclared symbol `{}`", symbol.0)
        }
        TermError::ArityMismatch {
            symbol,
            expected,
            actual,
        } => format!(
            "reference operator `{}` applied to {actual} argument(s), the capsule declares {expected}",
            symbol.0
        ),
        TermError::ConflictingArity {
            symbol,
            first,
            second,
        } => format!(
            "reference symbol `{}` is declared with conflicting arities {first} and {second}",
            symbol.0
        ),
    })?;
    let mut compiler = ReferenceCompiler {
        next_register: 0,
        ops: Vec::new(),
        obligations: Vec::new(),
        locals: BTreeMap::new(),
        params,
    };
    let result = compiler.emit(term)?;
    let mut program = EmirProgram {
        ops: compiler.ops,
        result,
        input_count: u16::try_from(params.len())
            .map_err(|_| "reference cells exceed u16::MAX parameters".to_string())?,
        state_count: 0,
        domain_obligations: compiler.obligations,
    };
    optimize::optimize_program(&mut program);
    Ok(CompiledCell {
        capability: capability.to_string(),
        params: params.to_vec(),
        defaults: Vec::new(),
        guards: Vec::new(),
        result_guard: None,
        program,
    })
}

fn compile_reference_with_defaults(
    term: &Term, defaults: &[Term], signature: &Signature,
    params: &[(String, ParamShape)], capability: &str,
) -> Result<CompiledCell, String> {
    let first = params.len().checked_sub(defaults.len())
        .ok_or_else(|| "reference defaults exceed parameter count".to_string())?;
    let mut cell = compile_reference_term(term, signature, params, capability)?;
    for (index, default) in defaults.iter().enumerate() {
        cell.defaults.push(compile_reference_term(default, signature, &params[..first + index], capability)?.program);
    }
    Ok(cell)
}

/// Lowers a validated canonical term onto the closed scalar vocabulary.
/// Registers are op indices; every op is stamped with a default span
/// because the authored source is capsule data, not a positioned file.
struct ReferenceCompiler<'a> {
    next_register: u32,
    ops: Vec<(EmirOp, Span)>,
    obligations: Vec<DomainObligation>,
    params: &'a [(String, ParamShape)],
    locals: BTreeMap<String, EmirValue>,
}

impl ReferenceCompiler<'_> {
    fn push(&mut self, op: EmirOp) -> EmirValue {
        let register = EmirValue(self.next_register);
        self.next_register += 1;
        self.ops.push((op, Span::default()));
        register
    }

    fn capture(&mut self) -> Vec<(String, EmirValue)> {
        for (index, (name, _)) in self.params.iter().enumerate() {
            if !self.locals.contains_key(name) {
                let value = self.push(EmirOp::LoadInput(index as u16));
                self.locals.insert(name.clone(), value);
            }
        }
        self.locals.iter().map(|(name, value)| (name.clone(), *value)).collect()
    }

    fn nested(term: &Term, names: Vec<String>) -> Result<EmirProgram, String> {
        let input_count = u16::try_from(names.len()).map_err(|_| "too many captured arguments".to_string())?;
        let params = names.into_iter().map(|name| (name, ParamShape::Scalar)).collect::<Vec<_>>();
        let mut compiler = ReferenceCompiler { next_register: 0, ops: Vec::new(), obligations: Vec::new(), params: &params, locals: BTreeMap::new() };
        let result = compiler.emit(term)?;
        Ok(EmirProgram { ops: compiler.ops, result, input_count, state_count: 0, domain_obligations: compiler.obligations })
    }

    fn emit_control(&mut self, symbol: &str, arguments: &[Term]) -> Result<Option<EmirValue>, String> {
        let literal = |term: &Term| match term {
            Term::Constant(symbol) => Ok(symbol.0.clone()),
            _ => Err("record fields and refusal codes require literal symbols".to_string()),
        };
        let op = match (symbol, arguments) {
            ("let", [Term::Variable(name), init, body]) => {
                let init = self.emit(init)?;
                let previous = self.locals.insert(name.0.clone(), init);
                let result = self.emit(body);
                if let Some(previous) = previous { self.locals.insert(name.0.clone(), previous); } else { self.locals.remove(&name.0); }
                return result.map(Some);
            }
            ("if", [condition, then_body, else_body]) => {
                let condition = self.emit(condition)?;
                let captured = self.capture();
                let names = captured.iter().map(|(name, _)| name.clone()).collect::<Vec<_>>();
                EmirOp::Branch { condition, args: captured.into_iter().map(|(_, value)| value).collect(), then_body: Self::nested(then_body, names.clone())?, else_body: Self::nested(else_body, names)? }
            }
            ("collect", [Term::Variable(index), count, body]) => {
                let count = self.emit(count)?;
                let captured = self.capture().into_iter().filter(|(name, _)| name != &index.0).collect::<Vec<_>>();
                let mut names = vec![index.0.clone()];
                names.extend(captured.iter().map(|(name, _)| name.clone()));
                EmirOp::Collect { count, args: captured.into_iter().map(|(_, value)| value).collect(), body: Self::nested(body, names)? }
            }
            (symbol, [Term::Variable(index), Term::Variable(state), count, init, rest @ ..]) if matches!(symbol, "iterate" | "iterate_until") => {
                let (body, stop) = match (symbol, rest) {
                    ("iterate", [body]) => (body, None),
                    ("iterate_until", [stop, body]) => (body, Some(stop)),
                    _ => return Err("bounded iteration has the wrong argument count".into()),
                };
                if index == state { return Err("iteration index and accumulator require distinct names".into()); }
                let count = self.emit(count)?;
                let init = self.emit(init)?;
                let captured = self.capture().into_iter().filter(|(name, _)| name != &index.0 && name != &state.0).collect::<Vec<_>>();
                let mut names = vec![index.0.clone(), state.0.clone()];
                names.extend(captured.iter().map(|(name, _)| name.clone()));
                let body = Self::nested(body, names.clone())?;
                let stop = stop.map(|stop| Self::nested(stop, names)).transpose()?;
                EmirOp::Iterate { count, init, args: captured.into_iter().map(|(_, value)| value).collect(), stop, body }
            }
            ("get", [record, field]) => EmirOp::RecordField { record: self.emit(record)?, field: literal(field)? },
            ("refuse", [detail]) => {
                let detail = literal(detail)?;
                EmirOp::Refuse(detail.strip_prefix("text:").unwrap_or(&detail).to_string())
            }
            ("refuse_value", [value]) => EmirOp::RefuseValue(self.emit(value)?),
            ("format_text", [template, arguments @ ..]) => {
                let template = literal(template)?;
                EmirOp::FormatText {
                    template: template.strip_prefix("text:").unwrap_or(&template).to_string(),
                    arguments: arguments.iter().map(|value| self.emit(value)).collect::<Result<Vec<_>, _>>()?,
                }
            }
            ("call_program", [program, inputs]) => EmirOp::CallProgram { program: self.emit(program)?, inputs: self.emit(inputs)? },
            ("call_scalar_program", [program, inputs]) => EmirOp::CallScalarProgram { program: self.emit(program)?, inputs: self.emit(inputs)? },
            ("call_real_program", [program, inputs]) => EmirOp::CallRealProgram { program: self.emit(program)?, inputs: self.emit(inputs)? },
            ("try_call_real_program", [program, inputs]) => EmirOp::TryCallRealProgram { program: self.emit(program)?, inputs: self.emit(inputs)? },
            ("result_is_ok", [value]) => EmirOp::ResultIsOk(self.emit(value)?),
            ("result_unwrap_or", [value, fallback]) => EmirOp::ResultUnwrapOr(self.emit(value)?, self.emit(fallback)?),
            ("vector", [value]) => EmirOp::ToF64Vector(self.emit(value)?),
            ("length", [value]) => EmirOp::VectorLength(self.emit(value)?),
            ("index", [value, index]) => EmirOp::VectorIndex { vector: self.emit(value)?, index: self.emit(index)? },
            ("list", elements) => EmirOp::ListCreate(elements.iter().map(|element| self.emit(element)).collect::<Result<Vec<_>, _>>()?),
            (symbol, values) if symbol.starts_with("record:") => {
                let mut parts = symbol[7..].split(':');
                let type_name = parts.next().unwrap_or_default().to_string();
                let fields = parts.collect::<Vec<_>>();
                if type_name.is_empty() || fields.len() != values.len() { return Err("record constructor requires a type and one field name per value".into()); }
                let mut seen = BTreeSet::new();
                let mut members = Vec::with_capacity(values.len());
                for (field, value) in fields.into_iter().zip(values) {
                    if field.is_empty() || !seen.insert(field) { return Err("record constructor has an empty or repeated field".into()); }
                    members.push((field.to_string(), self.emit(value)?));
                }
                EmirOp::RecordCreate { type_name, fields: members }
            }
            (symbol, values) if FeatureId::from_str(symbol).is_ok() => EmirOp::ApplyCapability { capability: symbol.to_string(), class: crate::CellClass::Pure, args: values.iter().map(|value| self.emit(value)).collect::<Result<Vec<_>, _>>()? },
            _ => return Ok(None),
        };
        Ok(Some(self.push(op)))
    }

    fn emit(&mut self, term: &Term) -> Result<EmirValue, String> {
        match term {
            Term::Variable(variable) => {
                if let Some(value) = self.locals.get(&variable.0) { return Ok(*value); }
                let index = self
                    .params
                    .iter()
                    .position(|(name, _)| name == &variable.0)
                    .ok_or_else(|| {
                        format!(
                            "reference term uses variable `{}` outside the declared parameter list",
                            variable.0
                        )
                    })?;
                let index = u16::try_from(index)
                    .map_err(|_| "reference cells exceed u16::MAX parameters".to_string())?;
                let value = self.push(EmirOp::LoadInput(index));
                self.locals.insert(variable.0.clone(), value);
                Ok(value)
            }
            Term::Constant(symbol) => {
                let op = match symbol.0.as_str() {
                    "true" => EmirOp::ConstBool(true),
                    "false" => EmirOp::ConstBool(false),
                    text if text.starts_with("text:") => EmirOp::ConstText(text[5..].to_string()),
                    text if text.starts_with("i64:") => {
                        EmirOp::ConstI64(text[4..].parse().map_err(|_| {
                            format!("reference integer literal `{text}` is outside i64")
                        })?)
                    }
                    text => {
                        let value = text
                            .parse::<f64>()
                            .ok()
                            .filter(|value| value.is_finite())
                            .ok_or_else(|| {
                                format!(
                                    "reference constant `{text}` is not a finite binary64 literal"
                                )
                            })?;
                        EmirOp::ConstF64(value.to_bits())
                    }
                };
                Ok(self.push(op))
            }
            Term::Apply {
                operator,
                arguments,
            } => {
                if let Some(value) = self.emit_control(&operator.0, arguments)? { return Ok(value); }
                let resolved = ReferenceOperator::resolve(&operator.0, arguments.len())
                    .ok_or_else(|| {
                        format!(
                            "reference operator `{}` with {} argument(s) is outside the \
                             closed machine-neutral scalar vocabulary",
                            operator.0,
                            arguments.len()
                        )
                    })?;
                let mut operands = Vec::with_capacity(arguments.len());
                for argument in arguments {
                    operands.push(self.emit(argument)?);
                }
                let op = resolved.lower(&operands, &mut self.obligations);
                Ok(self.push(op))
            }
        }
    }
}

/// Canonical page encoding: one stamped entry per capability, in feature
/// order. The printed program is length-prefixed so its embedded newlines
/// never confuse the line-oriented header parse.
fn encode_reference_partition(entries: &BTreeMap<FeatureId, ReferenceEntry>) -> String {
    if entries.is_empty() {
        return REFERENCE_NONE_PAGE.to_string();
    }
    let mut page = String::new();
    for (feature, entry) in entries {
        page.push_str("reference ");
        page.push_str(feature.as_str());
        page.push('\n');
        page.push_str("params ");
        for (index, (name, shape)) in entry.params.iter().enumerate() {
            if index > 0 {
                page.push(',');
            }
            page.push_str(name);
            page.push(':');
            page.push_str(shape.as_str());
        }
        page.push('\n');
        page.push_str(&format!("defaults {}\n", entry.defaults.len()));
        for default in &entry.defaults {
            page.push_str("default ");
            page.push_str(&default.canonical());
            page.push('\n');
        }
        page.push_str("term ");
        page.push_str(&entry.term.canonical());
        page.push('\n');
        let program = entry.cell.program.print();
        page.push_str(&format!("program {}\n", program.as_bytes().len()));
        page.push_str(&program);
    }
    page
}

fn decode_reference_entries(
    page: &str,
) -> Result<BTreeMap<FeatureId, ReferenceEntry>, LanguageImageError> {
    let malformed = |detail: String| LanguageImageError::ReferencePartitionMalformed(detail);
    if page == REFERENCE_NONE_PAGE {
        return Ok(BTreeMap::new());
    }
    let mut entries = BTreeMap::new();
    let mut rest = page;
    loop {
        let Some(line) = next_line(&mut rest) else {
            break;
        };
        let Some(feature_text) = line.strip_prefix("reference ") else {
            return Err(malformed(format!(
                "expected `reference <feature>` header, found `{line}`"
            )));
        };
        let feature = FeatureId::from_str(feature_text).map_err(|error| {
            malformed(format!(
                "reference key `{feature_text}` is not a feature id: {error}"
            ))
        })?;
        let params_line = next_line(&mut rest)
            .ok_or_else(|| malformed("reference entry ends before params".to_string()))?;
        let Some(params_text) = params_line.strip_prefix("params ") else {
            return Err(malformed(format!(
                "expected `params` line, found `{params_line}`"
            )));
        };
        let params = parse_partition_params(params_text).map_err(malformed)?;
        let defaults_line = next_line(&mut rest)
            .ok_or_else(|| malformed("reference entry ends before defaults".into()))?;
        let count = defaults_line.strip_prefix("defaults ").and_then(|text| text.parse::<usize>().ok())
            .filter(|count| *count <= params.len())
            .ok_or_else(|| malformed("invalid reference default count".into()))?;
        let mut defaults = Vec::with_capacity(count);
        for _ in 0..count {
            let text = next_line(&mut rest).and_then(|line| line.strip_prefix("default "))
                .ok_or_else(|| malformed("reference entry ends before a default term".into()))?;
            defaults.push(Term::parse_canonical(text).map_err(|error| malformed(format!("invalid reference default: {error:?}")))?);
        }
        let term_line = next_line(&mut rest)
            .ok_or_else(|| malformed("reference entry ends before term".to_string()))?;
        let Some(term_text) = term_line.strip_prefix("term ") else {
            return Err(malformed(format!(
                "expected `term` line, found `{term_line}`"
            )));
        };
        let term = Term::parse_canonical(term_text).map_err(|error| {
            malformed(format!(
                "reference term is not canonical emath-term text: {error:?}"
            ))
        })?;
        let program_line = next_line(&mut rest)
            .ok_or_else(|| malformed("reference entry ends before program".to_string()))?;
        let Some(length_text) = program_line.strip_prefix("program ") else {
            return Err(malformed(format!(
                "expected `program` line, found `{program_line}`"
            )));
        };
        let length: usize = length_text
            .parse()
            .map_err(|_| malformed(format!("program length `{length_text}` is not a size")))?;
        if rest.len() < length {
            return Err(malformed(
                "reference program bytes are truncated".to_string(),
            ));
        }
        let (program_text, remainder) = rest.split_at(length);
        rest = remainder;
        let mut signature = Signature::default();
        note_term_arities(&term, &mut signature)
            .map_err(|detail| malformed(format!("reference entry declares {detail}")))?;
        for default in &defaults {
            note_term_arities(default, &mut signature).map_err(|detail| malformed(format!("reference default declares {detail}")))?;
        }
        let cell = compile_reference_with_defaults(&term, &defaults, &signature, &params, feature.as_str()).map_err(
            |detail| {
                malformed(format!(
                    "reference entry `{}` refuses recompilation: {detail}",
                    feature.as_str()
                ))
            },
        )?;
        if cell.program.print() != program_text {
            return Err(LanguageImageError::ReferenceBytecodeMismatch {
                feature: feature.clone(),
            });
        }
        if entries
            .insert(feature, ReferenceEntry { term, defaults, params, cell })
            .is_some()
        {
            return Err(malformed(format!(
                "reference key `{feature_text}` appears twice"
            )));
        }
    }
    Ok(entries)
}

fn next_line<'a>(rest: &mut &'a str) -> Option<&'a str> {
    if rest.is_empty() {
        return None;
    }
    match rest.split_once('\n') {
        Some((line, remainder)) => {
            *rest = remainder;
            Some(line)
        }
        None => {
            let line = *rest;
            *rest = "";
            Some(line)
        }
    }
}

fn parse_partition_params(text: &str) -> Result<Vec<(String, ParamShape)>, String> {
    text.split(',')
        .map(|token| {
            let (name, shape) = token
                .trim()
                .split_once(':')
                .ok_or_else(|| format!("parameter `{token}` lacks a `name:shape` shape"))?;
            let shape = match shape.trim() {
                "scalar" => ParamShape::Scalar,
                "rational" => ParamShape::Rational,
                "vector" => ParamShape::Vector,
                "matrix" => ParamShape::Matrix,
                other => return Err(format!("unknown parameter shape `{other}`")),
            };
            Ok((name.trim().to_string(), shape))
        })
        .collect()
}

/// Derives the signature a decoded term actually uses, so recompilation
/// needs no capsule context: the page carries the full contract.
fn note_term_arities(term: &Term, signature: &mut Signature) -> Result<(), String> {
    match term {
        Term::Variable(_) => Ok(()),
        Term::Constant(symbol) => signature
            .insert(symbol.clone(), 0)
            .map_err(|error| format!("conflicting arities: {error:?}")),
        Term::Apply {
            operator,
            arguments,
        } => {
            signature
                .insert(operator.clone(), arguments.len())
                .map_err(|error| format!("conflicting arities: {error:?}"))?;
            for argument in arguments {
                note_term_arities(argument, signature)?;
            }
            Ok(())
        }
    }
}

/// Names the first capability where the installed public reference map
/// disagrees with the decoded `language.reference` partition: a changed
/// program, a missing capability, or an extra one. This is the mutation
/// proof for post-compile map edits — `verify()` never trusts the map.
fn first_installed_map_mismatch(
    installed: &BTreeMap<FeatureId, CompiledCell>,
    decoded: &BTreeMap<FeatureId, ReferenceEntry>,
) -> Option<FeatureId> {
    installed
        .iter()
        .find(|(feature, cell)| {
            decoded
                .get(feature)
                .map_or(true, |entry| &entry.cell != *cell)
        })
        .map(|(feature, _)| feature.clone())
        .or_else(|| {
            decoded
                .keys()
                .find(|feature| !installed.contains_key(feature))
                .cloned()
        })
}

fn reference_entries_agree(left: &ReferenceEntry, right: &ReferenceEntry) -> bool {
    left.term == right.term && left.defaults == right.defaults && left.params == right.params && left.cell == right.cell
}

/// Names the first capability whose compiled entry the decoded page does
/// not reproduce byte-for-byte. `None` only when the tables agree.
fn first_reference_mismatch(
    compiled: &BTreeMap<FeatureId, ReferenceEntry>,
    loaded: &BTreeMap<FeatureId, ReferenceEntry>,
) -> Option<FeatureId> {
    if compiled.len() == loaded.len()
        && compiled.iter().all(|(feature, entry)| {
            loaded
                .get(feature)
                .is_some_and(|other| reference_entries_agree(entry, other))
        })
    {
        return None;
    }
    compiled
        .iter()
        .find(|(feature, entry)| {
            !loaded
                .get(feature)
                .is_some_and(|other| reference_entries_agree(entry, other))
        })
        .map(|(feature, _)| feature.clone())
        .or_else(|| loaded.keys().next().cloned())
}
