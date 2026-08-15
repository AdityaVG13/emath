//! Typed identifiers for source files, schemas, content and names.

/// Identifier of a source file inside a compilation session.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FileId(pub u32);

/// Identifier of a durable schema version (for example `emath.artifact.v1`).
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
