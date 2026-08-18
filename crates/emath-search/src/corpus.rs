//! Artifact corpus model and the composite document-id encoding.
//!
//! Corpus records are artifact *metadata*: kind + id + optional path + claim
//! text. Encoding follows the skill's single-scheme rule — one
//! `to_fs_doc_id` / `from_fs_doc_id` pair, unit-separator composite keys, no
//! secondary id format anywhere in the crate.

use crate::error::SearchError;

/// Unit separator for composite frankensearch document ids (`kind \x1f id`).
///
/// Both parts must be non-empty and free of the separator, so `from_fs_doc_id`
/// is total over ids this crate produces.
pub const DOC_ID_SEPARATOR: char = '\x1f';

/// One artifact metadata record, the corpus unit for indexing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactDoc {
    /// Artifact id, caller-controlled (stable, sequence-ordered ids are the
    /// project convention; no wall clock).
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

#[cfg(test)]
mod tests {
    use super::{DOC_ID_SEPARATOR, from_fs_doc_id, to_fs_doc_id};
    use crate::ArtifactDoc;
    use crate::SearchError;

    #[test]
    fn round_trip_composite_id() {
        let encoded = to_fs_doc_id("artifact", "42").expect("encode");
        assert_eq!(encoded, format!("artifact{DOC_ID_SEPARATOR}42"));
        assert_eq!(
            from_fs_doc_id(&encoded),
            Some(("artifact".into(), "42".into()))
        );
    }

    #[test]
    fn empty_parts_rejected() {
        assert!(matches!(
            to_fs_doc_id("", "42"),
            Err(SearchError::InvalidArgument { field: "kind", .. })
        ));
        assert!(matches!(
            to_fs_doc_id("artifact", ""),
            Err(SearchError::InvalidArgument { field: "id", .. })
        ));
    }

    #[test]
    fn separator_inside_parts_rejected() {
        assert!(to_fs_doc_id("art\x1fifact", "42").is_err());
        assert!(to_fs_doc_id("artifact", "4\x1f2").is_err());
    }

    #[test]
    fn malformed_decode_returns_none() {
        assert_eq!(from_fs_doc_id(""), None);
        assert_eq!(from_fs_doc_id("no-separator"), None);
        assert_eq!(from_fs_doc_id("\x1f42"), None);
        assert_eq!(from_fs_doc_id("artifact\x1f"), None);
        assert_eq!(from_fs_doc_id("artifact\x1f4\x1f2"), None);
    }

    #[test]
    fn artifact_doc_validates_at_construction() {
        let doc = ArtifactDoc::new("7", "evidence", None, "verified claim").expect("valid");
        assert_eq!(doc.fs_doc_id().expect("id"), "evidence\x1f7");
        assert!(ArtifactDoc::new("7", "evi\x1fdence", None, "x").is_err());
    }
}
