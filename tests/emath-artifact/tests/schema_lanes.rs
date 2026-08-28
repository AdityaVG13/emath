//! Schema-id honesty lanes (cluster 2): each schema id names exactly one
//! in-tree writer shape, and each writer's document parses back against
//! its own shape while being refused by the sibling's reader.

use emath_artifact::{
    GeneratedCrateSourceMapEntry, OperationRecord, PlanRecord, SourceMap, SourceMapEntry,
    source_map_from_json, write_generated_crate_source_map, write_resolution_plan,
    write_source_map,
};
use emath_core::{SchemaId, content_id_of_str};

fn sample_source_map() -> SourceMap {
    SourceMap {
        schema: SchemaId("emath.source-map".into()),
        source_package: content_id_of_str("schema-lanes-source"),
        entries: vec![SourceMapEntry {
            file: 0,
            source_file: "spec.emath".to_string(),
            source_start: 0,
            source_end: 11,
            semantic_node: "y".to_string(),
            plan_node: Some("plan-0".to_string()),
            generated_file: "src/lib.rs".to_string(),
            generated_start: 4,
            generated_end: 9,
            generated_symbol: Some("Y".to_string()),
        }],
    }
}

/// Positive: the durable artifact source map parses back against its own
/// schema id and shape.
#[test]
fn artifact_source_map_round_trips_against_its_own_schema() {
    let map = sample_source_map();
    let doc = write_source_map(&map);
    assert!(doc.contains("\"schema\": \"emath.source-map\""));
    let parsed = source_map_from_json(&doc).expect("artifact source map must parse back");
    assert_eq!(parsed, map);
}

/// Positive + negative: the world-codegen provenance map parses back
/// against its own schema id, and the artifact source-map reader refuses
/// it (genesis bytes never load as the durable shape).
#[test]
fn generated_crate_source_map_round_trips_and_cross_load_is_refused() {
    let files = vec!["Cargo.toml".to_string(), "src/lib.rs".to_string()];
    let source = "/tmp/genesis/exa\"mple.emath";
    let doc = write_generated_crate_source_map(source, &files);
    assert!(doc.contains("\"schema\": \"emath.generated-crate-source-map\""));
    assert!(doc.contains("\"kind\": \"parametric-world\""));
    assert!(
        doc.contains(r#""source": "/tmp/genesis/exa\"mple.emath""#),
        "{doc}"
    );

    let parsed = emath_artifact::generated_crate_source_map_from_json(&doc)
        .expect("generated-crate map must parse back per its own schema");
    assert_eq!(
        parsed.schema,
        SchemaId("emath.generated-crate-source-map".into())
    );
    assert_eq!(parsed.source, source);
    assert_eq!(
        parsed.entries,
        vec![
            GeneratedCrateSourceMapEntry {
                generated: "Cargo.toml".to_string(),
                source: source.to_string(),
                kind: "parametric-world".to_string(),
            },
            GeneratedCrateSourceMapEntry {
                generated: "src/lib.rs".to_string(),
                source: source.to_string(),
                kind: "parametric-world".to_string(),
            },
        ]
    );

    assert!(
        source_map_from_json(&doc).is_err(),
        "genesis source-map bytes must be refused by the artifact source-map reader"
    );
}

/// Negative: the artifact source-map reader refuses a builder-level shape
/// mismatch, and the generated-crate reader refuses the durable shape.
#[test]
fn cross_schema_loading_is_refused_both_directions() {
    let artifact_doc = write_source_map(&sample_source_map());
    assert!(
        emath_artifact::generated_crate_source_map_from_json(&artifact_doc).is_err(),
        "artifact bytes must never load as the generated-crate map"
    );
}

/// Positive: the resolution-plan document round-trips against its own
/// schema id.
#[test]
fn resolution_plan_round_trips_against_its_own_schema() {
    let plan = PlanRecord {
        schema: SchemaId("emath.resolution-plan".into()),
        plan_id: content_id_of_str("schema-lanes-plan"),
        goal: 0,
        policy: "native-deterministic".to_string(),
        artifact_class: "native".to_string(),
        operations: vec![OperationRecord {
            node: 0,
            operation: "package".to_string(),
            dependencies: Vec::new(),
            fallback: None,
        }],
        excluded_candidates: vec![("phase2.expression".to_string(), "not installed".to_string())],
    };
    let doc = write_resolution_plan(&plan);
    assert!(doc.contains("\"schema\": \"emath.resolution-plan\""));
    let parsed = emath_artifact::plan_from_json(&doc).expect("plan must parse back");
    assert_eq!(parsed, plan);
}
