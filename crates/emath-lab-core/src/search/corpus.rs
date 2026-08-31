//! Artifact corpus model and the composite document-id encoding (one
//! `to_fs_doc_id` / `from_fs_doc_id` pair, unit-separator composite keys).

use crate::search::error::SearchError;

/// Unit separator for composite frankensearch document ids (`kind \x1f id`);
/// both parts must be non-empty and free of the separator.
pub const DOC_ID_SEPARATOR: char = '\x1f';

/// One artifact metadata record, the corpus unit for indexing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactDoc {
    /// Artifact id, caller-controlled (sequence-ordered; no wall clock).
    pub id: String,
    /// Artifact kind (e.g. `goal`, `plan`, `artifact`, `evidence`).
    pub kind: String,
    /// Optional artifact path carried through to search hits.
    pub path: Option<String>,
    /// Claim text: the searchable content of the record.
    pub text: String,
}

impl ArtifactDoc {
    /// Construct with validation. Errors when the composite document id
    /// cannot be encoded: empty or separator-containing `kind`/`id`.
    pub fn new(
        id: impl Into<String>,
        kind: impl Into<String>,
        path: Option<String>,
        text: impl Into<String>,
    ) -> Result<Self, SearchError> {
        let doc = ArtifactDoc {
            id: id.into(),
            kind: kind.into(),
            path,
            text: text.into(),
        };
        doc.fs_doc_id().map(|_| doc)
    }

    /// The frankensearch document id for this record (`kind \x1f id`).
    pub fn fs_doc_id(&self) -> Result<String, SearchError> {
        to_fs_doc_id(&self.kind, &self.id)
    }
}

/// Encode `kind` and `id` into the single frankensearch document id scheme.
pub fn to_fs_doc_id(kind: &str, id: &str) -> Result<String, SearchError> {
    if kind.is_empty() {
        return Err(SearchError::InvalidArgument {
            field: "kind",
            reason: "must be non-empty".into(),
        });
    }
    if id.is_empty() {
        return Err(SearchError::InvalidArgument {
            field: "id",
            reason: "must be non-empty".into(),
        });
    }
    if kind.contains(DOC_ID_SEPARATOR) || id.contains(DOC_ID_SEPARATOR) {
        return Err(SearchError::InvalidArgument {
            field: "doc_id",
            reason: format!("neither kind nor id may contain the separator {DOC_ID_SEPARATOR:?}"),
        });
    }
    Ok(format!("{kind}{DOC_ID_SEPARATOR}{id}"))
}

/// Decode a composite id produced by [`to_fs_doc_id`]. `None` on any
/// malformed input (missing separator, empty part, embedded separator).
pub fn from_fs_doc_id(fs_doc_id: &str) -> Option<(String, String)> {
    let (kind, id) = fs_doc_id.split_once(DOC_ID_SEPARATOR)?;
    if kind.is_empty() || id.is_empty() {
        return None;
    }
    if kind.contains(DOC_ID_SEPARATOR) || id.contains(DOC_ID_SEPARATOR) {
        return None;
    }
    Some((kind.to_string(), id.to_string()))
}
