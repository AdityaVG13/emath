//! Typed identifiers for source files, schemas, content and names.

use std::fmt;
use std::str::FromStr;

/// Identifier of a source file inside a compilation session.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FileId(pub u32);

/// Identifier of a durable schema (for example `emath.artifact`).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SchemaId(pub String);

/// Content identity (bootstrap fingerprint; see `crate::hash`).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContentId(pub String);

/// A path of identifiers, for example `core::math::Real`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct QualifiedName(pub String);

impl QualifiedName {
    #[must_use]
    pub fn single(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// Last path segment, e.g. `Real` for `core::math::Real`.
    #[must_use]
    pub fn leaf(&self) -> &str {
        self.0.rsplit("::").next().unwrap_or(&self.0)
    }

    #[must_use]
    pub fn join(&self, name: impl AsRef<str>) -> Self {
        Self(format!("{}::{}", self.0, name.as_ref()))
    }
}

/// A malformed durable identity string.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdentityParseError {
    expected_prefix: &'static str,
}

impl IdentityParseError {
    #[must_use]
    pub fn expected_prefix(&self) -> &'static str {
        self.expected_prefix
    }
}

impl fmt::Display for IdentityParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "identity must be `{}` followed by 64 lowercase hexadecimal digits",
            self.expected_prefix
        )
    }
}

impl std::error::Error for IdentityParseError {}

fn parse_identity(
    value: &str,
    expected_prefix: &'static str,
) -> Result<String, IdentityParseError> {
    let digest = value
        .strip_prefix(expected_prefix)
        .filter(|digest| digest.len() == 64)
        .filter(|digest| {
            digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        })
        .ok_or(IdentityParseError { expected_prefix })?;
    debug_assert_eq!(digest.len(), 64);
    Ok(value.to_string())
}

macro_rules! durable_identity_type {
    ($name:ident, $domain:literal) => {
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            pub const PREFIX: &'static str = concat!("emath:", $domain, ":v1:");

            #[must_use]
            pub fn from_bytes(bytes: &[u8]) -> Self {
                Self(crate::hash::durable_identity(Self::PREFIX, bytes))
            }

            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }

            #[must_use]
            pub fn into_string(self) -> String {
                self.0
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(self.as_str())
            }
        }

        impl FromStr for $name {
            type Err = IdentityParseError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                parse_identity(value, Self::PREFIX).map(Self)
            }
        }
    };
}

durable_identity_type!(SourceId, "source");
durable_identity_type!(MeaningId, "meaning");
durable_identity_type!(EvidenceId, "evidence");
durable_identity_type!(ViewId, "view");
durable_identity_type!(RecipeId, "recipe");
durable_identity_type!(ArtifactId, "artifact");
durable_identity_type!(SnapshotId, "snapshot");
durable_identity_type!(PackId, "pack");
durable_identity_type!(ObjectId, "object");
durable_identity_type!(RelationId, "relation");
durable_identity_type!(MergeId, "merge");
