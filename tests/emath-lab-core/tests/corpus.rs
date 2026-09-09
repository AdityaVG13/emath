//! Search artifact-corpus id tests.

use emath_lab_core::search::{ArtifactDoc, DOC_ID_SEPARATOR, SearchError, from_fs_doc_id, to_fs_doc_id};
use emath_test_harness::{Case, Probe, check_all, expect_ok};

#[test]
fn artifact_corpus_ids() {
    let mut p = Probe::new("composite doc ids round-trip and refuse malformed parts");
    p.case("round-trip", |p| {
        let encoded = to_fs_doc_id("artifact", "42").expect("encode");
        p.eq("encoded", encoded.clone(), format!("artifact{DOC_ID_SEPARATOR}42"));
        p.eq("decoded", from_fs_doc_id(&encoded), Some(("artifact".into(), "42".into())));
    });
    p.case("empty-refused", |p| {
        p.demand("kind", matches!(to_fs_doc_id("", "42"), Err(SearchError::InvalidArgument { field: "kind", .. })), "empty kind refused");
        p.demand("id", matches!(to_fs_doc_id("artifact", ""), Err(SearchError::InvalidArgument { field: "id", .. })), "empty id refused");
    });
    p.case("separator-refused", |p| {
        expect_ok(check_all(
            &[Case::new("kind", ("art\x1fifact", "42"), true), Case::new("id", ("artifact", "4\x1f2"), true)],
            |input: &(&str, &str)| to_fs_doc_id(input.0, input.1).is_err(),
        ));
    });
    p.case("malformed-decode", |p| {
        expect_ok(check_all(
            &[Case::new("empty", "", None), Case::new("plain", "no-separator", None), Case::new("nokind", "\x1f42", None), Case::new("noid", "artifact\x1f", None), Case::new("extra", "artifact\x1f4\x1f2", None)],
            |input: &&str| from_fs_doc_id(input).map(|(a, b)| (a.to_string(), b.to_string())),
        ));
    });
    p.case("doc-validates", |p| {
        let doc = ArtifactDoc::new("7", "evidence", None, "verified claim").expect("valid");
        p.eq("id", doc.fs_doc_id().expect("id"), "evidence\x1f7");
        p.demand("separator-refused", ArtifactDoc::new("7", "evi\x1fdence", None, "x").is_err(), "separator in kind refused");
    });
    p.finish();
}
