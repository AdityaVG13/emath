//! Parse-forest schema conformance: the `emath.parse-forest.v1` document
//! written by `ParseForest::canonical_json` must parse back against its
//! own schema shape, and no other schema id may claim the document.

use emath_artifact::parse_json_document;
use emath_genesis::forest::{ForestLimits, build_forest};

const FOREST_SCHEMA: &str = "emath.parse-forest.v1";

fn parse_back(doc: &str) -> emath_artifact::JsonValue {
    parse_json_document(doc).expect("canonical_json must parse as JSON")
}

/// Positive: the emitted document carries exactly the schema's required
/// fields with the expected types.
#[test]
fn canonical_json_parses_back_against_its_schema_shape() {
    let forest = build_forest("f(a b)", &ForestLimits::default());
    let doc = forest.canonical_json();
    let root = parse_back(&doc);

    assert_eq!(
        root.string_field("schema").unwrap(),
        FOREST_SCHEMA,
        "schema const must be {FOREST_SCHEMA}"
    );
    assert_eq!(root.string_field("world_name").unwrap(), "");
    assert_eq!(root.string_field("body").unwrap(), "f(a b)");
    let parse_id = root.int_field("parse_id").unwrap();
    assert_ne!(parse_id, 0, "parse_id must be present in canonical_json");
    // The document must carry the writer's own reported values
    // (shape + consistency; no magic-number assumptions).
    let doc_ambiguity: usize = root
        .int_field("ambiguity_count")
        .unwrap()
        .try_into()
        .expect("ambiguity_count fits usize");
    assert_eq!(doc_ambiguity, forest.ambiguity_count());
    let doc_nodes: usize = root
        .int_field("node_count")
        .unwrap()
        .try_into()
        .expect("node_count fits usize");
    assert_eq!(doc_nodes, forest.node_count());
    assert_eq!(root.string_field("recovery").unwrap(), "bounded-holes");
    // holes is an array; the writer may emit empty
    assert!(matches!(
        root.field("holes").unwrap(),
        emath_artifact::JsonValue::Arr(_)
    ));
}

/// Negative: the durable artifact source-map reader must refuse the
/// parse-forest document; one schema id never names two shapes.
#[test]
fn forest_document_is_rejected_by_artifact_source_map_reader() {
    let forest = build_forest("f(a b)", &ForestLimits::default());
    let doc = forest.canonical_json();
    assert!(
        emath_artifact::source_map_from_json(&doc).is_err(),
        "emath.parse-forest.v1 bytes must not load as emath.source-map.v1"
    );
    assert!(
        !doc.contains("emath.source-map.v1"),
        "the forest document must claim its own schema id"
    );
}

/// Negative: two genesis documents share no schema id.
#[test]
fn forest_and_artifact_schema_ids_are_disjoint() {
    let forest = build_forest("f(a b)", &ForestLimits::default());
    let doc = forest.canonical_json();
    let root = parse_back(&doc);
    assert_ne!(
        root.string_field("schema").unwrap(),
        "emath.source-map.v1",
        "forest bytes must never claim the artifact source-map id"
    );
}
