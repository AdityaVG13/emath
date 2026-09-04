//! Durable FeatureIDs and separated language hash domains.
//!
//! FeatureIDs name stable language concepts. Exact meaning, distributable bytes,
//! and operational provenance are named by separate SHA-256 envelopes.

use std::fmt;
use std::str::FromStr;

use unicode_normalization::UnicodeNormalization;

const SEMANTIC_FRAME: &[u8] = b"emath.feature.semantic\0";
const DISTRIBUTION_FRAME: &[u8] = b"emath.feature.distribution\0";
const OPERATIONAL_FRAME: &[u8] = b"emath.feature.operational\0";

/// Stable, unversioned language concept name: `<authority>.<class>.<path>`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FeatureId(String);

impl FeatureId {
    /// Canonical identifier bytes.
    #[must_use]
    pub fn canonical_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }

    /// Canonical identifier text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Authority segment.
    #[must_use]
    pub fn authority(&self) -> &str {
        self.0.split('.').next().expect("validated FeatureId")
    }

    /// Primary feature-class segment.
    #[must_use]
    pub fn class(&self) -> &str {
        self.0.split('.').nth(1).expect("validated FeatureId")
    }

    /// Path segments after authority and class.
    pub fn path(&self) -> impl Iterator<Item = &str> {
        self.0.split('.').skip(2)
    }

    /// Require the ID's class to match its capsule's primary class.
    pub fn require_class(&self, primary_class: &str) -> Result<(), FeatureIdError> {
        if self.class() == primary_class {
            Ok(())
        } else {
            Err(FeatureIdError::new(FeatureIdErrorKind::ClassMismatch))
        }
    }
}

impl fmt::Display for FeatureId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for FeatureId {
    type Err = FeatureIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.nfc().ne(value.chars()) {
            return Err(FeatureIdError::new(FeatureIdErrorKind::NotNfc));
        }
        if value.bytes().any(|byte| byte.is_ascii_uppercase()) {
            return Err(FeatureIdError::new(FeatureIdErrorKind::Uppercase));
        }
        if !value.is_ascii() {
            return Err(FeatureIdError::new(FeatureIdErrorKind::NonAscii));
        }
        if value.contains('@') {
            return Err(FeatureIdError::new(if has_numeric_suffix(value) {
                FeatureIdErrorKind::VersionSuffix
            } else {
                FeatureIdErrorKind::InvalidCharacter
            }));
        }

        let mut segments = value.split('.');
        let Some(authority) = segments.next() else {
            return Err(FeatureIdError::new(FeatureIdErrorKind::MissingPath));
        };
        let Some(class) = segments.next() else {
            return Err(FeatureIdError::new(FeatureIdErrorKind::MissingPath));
        };
        let Some(first_path) = segments.next() else {
            return Err(FeatureIdError::new(FeatureIdErrorKind::MissingPath));
        };
        for segment in [authority, class, first_path].into_iter().chain(segments) {
            if segment.is_empty() {
                return Err(FeatureIdError::new(FeatureIdErrorKind::EmptySegment));
            }
            let mut bytes = segment.bytes();
            if !bytes.next().is_some_and(|byte| byte.is_ascii_lowercase()) {
                return Err(FeatureIdError::new(FeatureIdErrorKind::InvalidStart));
            }
            if !bytes.all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
            }) {
                return Err(FeatureIdError::new(FeatureIdErrorKind::InvalidCharacter));
            }
        }
        Ok(Self(value.to_string()))
    }
}

fn has_numeric_suffix(value: &str) -> bool {
    value.rsplit_once('@').is_some_and(|(_, suffix)| {
        !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
    })
}

/// FeatureID refusal category.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeatureIdErrorKind {
    NotNfc,
    Uppercase,
    NonAscii,
    VersionSuffix,
    MissingPath,
    EmptySegment,
    InvalidStart,
    InvalidCharacter,
    ClassMismatch,
}

/// Invalid FeatureID.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeatureIdError {
    kind: FeatureIdErrorKind,
}

impl FeatureIdError {
    const fn new(kind: FeatureIdErrorKind) -> Self {
        Self { kind }
    }

    #[must_use]
    pub const fn kind(&self) -> FeatureIdErrorKind {
        self.kind
    }
}

impl fmt::Display for FeatureIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid FeatureID: {:?}", self.kind)
    }
}

impl std::error::Error for FeatureIdError {}

/// One field in a canonical hash envelope.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CanonicalField<'a> {
    name: &'a str,
    value: &'a [u8],
}

impl<'a> CanonicalField<'a> {
    pub fn new(name: &'a str, value: &'a [u8]) -> Result<Self, HashEnvelopeError> {
        validate_field_name(name)?;
        Ok(Self { name, value })
    }
}

fn validate_field_name(name: &str) -> Result<(), HashEnvelopeError> {
    if name.is_empty()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err(HashEnvelopeError::new(
            name,
            "field names use lowercase ASCII snake_case",
        ));
    }
    if name
        .split('_')
        .any(|part| matches!(part, "version" | "edition" | "major" | "minor" | "patch"))
    {
        return Err(HashEnvelopeError::new(name, "version fields are forbidden"));
    }
    Ok(())
}

/// Canonical language hash domain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HashDomain {
    Semantic,
    Distribution,
    Operational,
}

impl HashDomain {
    const fn frame(self) -> &'static [u8] {
        match self {
            Self::Semantic => SEMANTIC_FRAME,
            Self::Distribution => DISTRIBUTION_FRAME,
            Self::Operational => OPERATIONAL_FRAME,
        }
    }
}

/// Invalid field placement or malformed canonical hash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HashEnvelopeError {
    field: String,
    detail: &'static str,
}

impl HashEnvelopeError {
    fn new(field: &str, detail: &'static str) -> Self {
        Self {
            field: field.to_string(),
            detail,
        }
    }

    #[must_use]
    pub fn field(&self) -> &str {
        &self.field
    }
}

impl fmt::Display for HashEnvelopeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid canonical field `{}`: {}",
            self.field, self.detail
        )
    }
}

impl std::error::Error for HashEnvelopeError {}

fn is_operational_field(name: &str) -> bool {
    name == "path"
        || name.starts_with("repository_")
        || name.starts_with("binary_")
        || name.starts_with("measurement_")
        || name.starts_with("timestamp")
        || name.starts_with("receipt_")
        || name.starts_with("agent_")
}

fn is_semantic_field(name: &str) -> bool {
    matches!(
        name,
        "semantics"
            | "semantic_edge"
            | "semantic_edges"
            | "feature_id"
            | "class"
            | "laws"
            | "types"
            | "units"
            | "shapes"
            | "domains"
    )
}

fn canonical_digest(
    domain: HashDomain,
    fields: &[CanonicalField<'_>],
) -> Result<[u8; 32], HashEnvelopeError> {
    let mut ordered = fields.iter().collect::<Vec<_>>();
    ordered.sort_unstable_by_key(|field| field.name);
    for pair in ordered.windows(2) {
        if pair[0].name == pair[1].name {
            return Err(HashEnvelopeError::new(pair[0].name, "duplicate field"));
        }
    }
    for field in &ordered {
        match domain {
            HashDomain::Semantic if is_operational_field(field.name) => {
                return Err(HashEnvelopeError::new(
                    field.name,
                    "operational metadata cannot enter semantic identity",
                ));
            }
            HashDomain::Operational if is_semantic_field(field.name) => {
                return Err(HashEnvelopeError::new(
                    field.name,
                    "semantic metadata cannot enter operational identity",
                ));
            }
            _ => {}
        }
    }

    let mut bytes = Vec::new();
    bytes.extend_from_slice(domain.frame());
    bytes.extend_from_slice(&(ordered.len() as u64).to_be_bytes());
    for field in ordered {
        bytes.extend_from_slice(&(field.name.len() as u64).to_be_bytes());
        bytes.extend_from_slice(field.name.as_bytes());
        bytes.extend_from_slice(&(field.value.len() as u64).to_be_bytes());
        bytes.extend_from_slice(field.value);
    }
    Ok(crate::hash::sha256_digest(&bytes))
}

fn digest_hex(digest: &[u8; 32]) -> String {
    use fmt::Write as _;
    let mut text = String::with_capacity(64);
    for byte in digest {
        let _ = write!(text, "{byte:02x}");
    }
    text
}

macro_rules! hash_type {
    ($name:ident, $domain:ident, $prefix:literal) => {
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            pub fn new(fields: &[CanonicalField<'_>]) -> Result<Self, HashEnvelopeError> {
                let digest = canonical_digest(HashDomain::$domain, fields)?;
                Ok(Self(format!(concat!($prefix, "{}"), digest_hex(&digest))))
            }

            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }

            #[must_use]
            pub const fn domain(&self) -> HashDomain {
                HashDomain::$domain
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }

        impl FromStr for $name {
            type Err = HashEnvelopeError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                let digest = value
                    .strip_prefix($prefix)
                    .filter(|digest| digest.len() == 64)
                    .filter(|digest| {
                        digest
                            .bytes()
                            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                    })
                    .ok_or_else(|| {
                        HashEnvelopeError::new("hash", "wrong domain or malformed SHA-256 hash")
                    })?;
                debug_assert_eq!(digest.len(), 64);
                Ok(Self(value.to_string()))
            }
        }
    };
}

hash_type!(SemanticHash, Semantic, "sha256:");
hash_type!(DistributionHash, Distribution, "distribution-sha256:");
hash_type!(OperationalHash, Operational, "operational-sha256:");

/// Explicit tag for a pre-cutover identifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LegacyIdKind {
    Fnv1a64,
    BootstrapContent,
}

impl LegacyIdKind {
    const fn tag(self) -> &'static str {
        match self {
            Self::Fnv1a64 => "fnv1a64",
            Self::BootstrapContent => "bootstrap-content",
        }
    }
}

/// A legacy identifier that cannot be confused with a FeatureID.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyId {
    kind: LegacyIdKind,
    value: String,
}

impl LegacyId {
    pub fn new(kind: LegacyIdKind, value: impl Into<String>) -> Result<Self, LegacyMappingError> {
        let value = value.into();
        if value.is_empty()
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(LegacyMappingError);
        }
        Ok(Self { kind, value })
    }
}

/// Explicit, reversible legacy-ID to FeatureID mapping.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyIdMapping {
    legacy_id: LegacyId,
    feature_id: FeatureId,
}

impl LegacyIdMapping {
    #[must_use]
    pub fn new(legacy_id: LegacyId, feature_id: FeatureId) -> Self {
        Self {
            legacy_id,
            feature_id,
        }
    }

    #[must_use]
    pub fn canonical_text(&self) -> String {
        format!(
            "legacy_id={}:{}\nfeature_id={}\n",
            self.legacy_id.kind.tag(),
            self.legacy_id.value,
            self.feature_id
        )
    }
}

impl FromStr for LegacyIdMapping {
    type Err = LegacyMappingError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let mut lines = value.lines();
        let legacy = lines
            .next()
            .and_then(|line| line.strip_prefix("legacy_id="))
            .ok_or(LegacyMappingError)?;
        let feature = lines
            .next()
            .and_then(|line| line.strip_prefix("feature_id="))
            .ok_or(LegacyMappingError)?;
        if lines.next().is_some() {
            return Err(LegacyMappingError);
        }
        let (tag, raw) = legacy.split_once(':').ok_or(LegacyMappingError)?;
        let kind = match tag {
            "fnv1a64" => LegacyIdKind::Fnv1a64,
            "bootstrap-content" => LegacyIdKind::BootstrapContent,
            _ => return Err(LegacyMappingError),
        };
        let legacy_id = LegacyId::new(kind, raw).map_err(|_| LegacyMappingError)?;
        let feature_id = FeatureId::from_str(feature).map_err(|_| LegacyMappingError)?;
        Ok(Self {
            legacy_id,
            feature_id,
        })
    }
}

/// Malformed or untagged legacy mapping.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LegacyMappingError;

impl fmt::Display for LegacyMappingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("legacy identifiers require an explicit recognized kind tag")
    }
}

impl std::error::Error for LegacyMappingError {}
