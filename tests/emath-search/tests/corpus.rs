//! `emath-search` artifact-corpus tests (migrated from
//! `crates/emath-search/src/corpus.rs`).

use emath_search::{ArtifactDoc, DOC_ID_SEPARATOR, SearchError, from_fs_doc_id, to_fs_doc_id};

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
