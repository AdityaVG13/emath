//! Schema-registry witnesses: fixed thirteen-schema enumeration, stable
//! version constants, deterministic byte-stable JSON documents with
//! matching examples, pairwise distinct bodies, closed-world emitter
//! properties, stable typed unknown-name refusal and known-name checks.

use emath_schema::{
    REGISTRY_VERSION, SCHEMAS_VERSION, SCHEMA_NAMES, SCHEMA_VERSION, VERSION, SchemaError,
    example_json, example_json_bytes, example_json_string, is_known_schema, schema_json,
    schema_json_bytes, schema_json_string, schema_names, write_example_json, write_schema_json,
};

#[test]
fn registry_enumerates_thirteen_in_fixed_order() {
    let names = schema_names();
    assert_eq!(
        names.len(),
        13,
        "registry must contain exactly thirteen schemas"
    );
    assert_eq!(names, &SCHEMA_NAMES, "fixed order must equal SCHEMA_NAMES");
    assert_eq!(schema_names(), names);
    let mut seen = std::collections::BTreeSet::new();
    for n in names {
        assert!(seen.insert(*n), "duplicate name {n}");
    }
}

#[test]
fn version_constants_stable() {
    assert_eq!(SCHEMA_VERSION, "1.0.0");
    assert_eq!(REGISTRY_VERSION, "1.0.0");
    assert_eq!(SCHEMAS_VERSION, "1.0.0");
    assert_eq!(VERSION, "1.0.0");
}

#[test]
fn every_entry_emits_deterministic_valid_json_and_matching_example() {
    for name in schema_names() {
        let schema = schema_json(name).expect("known schema");
        let example = example_json(name).expect("known example");
        assert_eq!(
            schema,
            schema_json(name).unwrap(),
            "schema not byte-stable for {name}",
        );
        assert_eq!(
            example,
            example_json(name).unwrap(),
            "example not byte-stable for {name}",
        );
        assert!(
            schema.starts_with(b"{"),
            "schema must start with {{ for {name}",
        );
        assert!(
            schema.ends_with(b"\n"),
            "schema must end with newline for {name}",
        );
        assert!(
            example.starts_with(b"{"),
            "example must start with {{ for {name}",
        );
        let schema_str = String::from_utf8(schema.clone()).unwrap();
        let example_str = String::from_utf8(example.clone()).unwrap();
        assert!(
            schema_str.contains(&format!("\"$id\": \"{name}\"")),
            "schema $id must equal {name}",
        );
        assert!(
            schema_str.contains(SCHEMA_VERSION),
            "schema document must contain version"
        );
        assert!(
            schema_str.contains("\"$schema\""),
            "schema must contain $schema"
        );
        assert!(
            example_str.contains(&format!("\"$schema\": \"{name}\"")),
            "example $schema must equal schema $id for {name}",
        );
        assert_eq!(schema, schema_json_bytes(name).unwrap());
        assert_eq!(example, example_json_bytes(name).unwrap());
        let mut buf = Vec::new();
        write_schema_json(name, &mut buf).unwrap();
        assert_eq!(buf, schema);
        let mut buf2 = Vec::new();
        write_example_json(name, &mut buf2).unwrap();
        assert_eq!(buf2, example);
        assert_eq!(
            schema_json_string(name).unwrap().as_bytes(),
            schema.as_slice()
        );
        assert_eq!(
            example_json_string(name).unwrap().as_bytes(),
            example.as_slice()
        );
    }
}

#[test]
fn thirteen_schema_documents_are_pairwise_distinct() {
    let docs: Vec<Vec<u8>> = schema_names()
        .iter()
        .map(|name| schema_json(name).expect("known schema"))
        .collect();
    for (i, left) in docs.iter().enumerate() {
        for (j, right) in docs.iter().enumerate().skip(i + 1) {
            assert_ne!(
                left, right,
                "schema bodies must differ: {} vs {}",
                SCHEMA_NAMES[i], SCHEMA_NAMES[j]
            );
        }
    }
}

#[test]
fn closed_world_schemas_are_not_one_template() {
    fn without_identity(name: &str) -> String {
        schema_json_string(name)
            .unwrap()
            .lines()
            .filter(|line| {
                !line.contains("\"$id\"")
                    && !line.contains("\"title\"")
                    && !line.contains("\"description\"")
            })
            .collect()
    }
    let source = without_identity("emath.source-artifact");
    let forest = without_identity("emath.parse-forest");
    let receipt = without_identity("emath.answer-receipt");
    let portfolio = without_identity("emath.interpretation-portfolio");
    let envelope = without_identity("emath.meaning-lock");
    assert_ne!(source, forest);
    assert_ne!(source, receipt);
    assert_ne!(source, envelope);
    assert_ne!(forest, receipt);
    assert_ne!(forest, envelope);
    assert_ne!(receipt, portfolio);
    assert_ne!(portfolio, envelope);
}

#[test]
fn source_artifact_properties_match_genesis_emitter() {
    let schema = schema_json_string("emath.source-artifact").unwrap();
    for field in [
        "schema_version",
        "source",
        "source_hash",
        "byte_len",
        "world_name",
        "body_text",
        "glyph_count",
        "glyphs",
        "parse_id",
    ] {
        assert!(
            schema.contains(&format!("\"{field}\"")),
            "source-artifact schema missing emitter field {field}"
        );
    }
    assert!(schema.contains("\"additionalProperties\": false"));
    assert!(!schema.contains("\"payload\""));
}

#[test]
fn parse_forest_properties_match_canonical_json_emitter() {
    let schema = schema_json_string("emath.parse-forest").unwrap();
    for field in [
        "world_name",
        "body",
        "parse_id",
        "ambiguity_count",
        "node_count",
        "holes",
        "canonical_term",
        "recovery",
    ] {
        assert!(
            schema.contains(&format!("\"{field}\"")),
            "parse-forest schema missing emitter field {field}"
        );
    }
    assert!(schema.contains("\"additionalProperties\": false"));
    assert!(schema.contains("\"const\": \"bounded-holes\""));
}

#[test]
fn answer_receipt_properties_match_genesis_emitter() {
    let schema = schema_json_string("emath.answer-receipt").unwrap();
    for field in [
        "receipt_id",
        "answer_id",
        "source_hash",
        "parse_id",
        "signature_id",
        "term_id",
        "world_id",
        "valuation",
        "provider_locks",
        "checker_receipts",
        "artifact_hash",
        "portfolio_hash",
        "target",
        "result",
        "trace_hash",
        "authority",
        "vm_schema",
        "vm_steps",
    ] {
        assert!(
            schema.contains(&format!("\"{field}\"")),
            "answer-receipt schema missing emitter field {field}"
        );
    }
    assert!(schema.contains("\"additionalProperties\": false"));
}

#[test]
fn envelope_schemas_do_not_invent_fields() {
    let schema = schema_json_string("emath.meaning-lock").unwrap();
    assert!(schema.contains("\"additionalProperties\": true"));
    assert!(!schema.contains("\"payload\""));
    assert!(!schema.contains("\"world_name\""));
    assert!(schema.contains("\"const\": \"emath.meaning-lock\""));
}

#[test]
fn unknown_names_return_stable_typed_error() {
    let unknown = "emath.unknown";
    let err = schema_json(unknown).unwrap_err();
    assert_eq!(err.code(), "E-SCHEMA-001");
    assert_eq!(err.code, "E-SCHEMA-001");
    assert_eq!(err.name(), unknown);
    assert_eq!(err.name, unknown);
    let err2 = example_json(unknown).unwrap_err();
    assert_eq!(err2.code(), "E-SCHEMA-001");
    assert_eq!(err2.name(), unknown);
    for bad in ["", "unknown", "emath.parse-forest.x", "EMATH.PARSE-FOREST"] {
        assert!(schema_json(bad).is_err(), "should refuse {bad}");
        assert!(example_json(bad).is_err(), "should refuse {bad}");
        let e = schema_json(bad).unwrap_err();
        assert_eq!(e.code, SchemaError::CODE);
    }
    let display = format!("{err}");
    assert!(display.contains(unknown));
    assert!(display.contains("E-SCHEMA-001"));
    let _: &dyn std::error::Error = &err;
}

#[test]
fn is_known_schema_matches_registry() {
    for name in schema_names() {
        assert!(is_known_schema(name));
    }
    assert!(!is_known_schema("emath.unknown"));
    assert!(!is_known_schema(""));
}
