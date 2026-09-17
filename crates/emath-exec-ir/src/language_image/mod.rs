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
    "std.capability.sequence",
    "std.capability.refusal",
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


// Split out of the original single file. `prelude` reexports the
// children at module width so they can share helpers; the `pub use`
// lines preserve the original crate-visible surface.
use prelude::*;
mod prelude;
mod authority;
mod codec;
mod compile;
mod image;
mod reference;

pub use compile::*;

