//! Schema-id honesty lanes: each schema id names exactly one writer shape.
use emath_artifact::{GeneratedCrateSourceMapEntry, OperationRecord, PlanRecord, SourceMap, SourceMapEntry, source_map_from_json, write_generated_crate_source_map, write_resolution_plan, write_source_map};
use emath_core::{SchemaId, content_id_of_str};
use emath_test_harness::Probe;

fn sample_source_map() -> SourceMap {
    SourceMap {
        schema: SchemaId("emath.source-map".into()),
        source_package: content_id_of_str("schema-lanes-source"),
        entries: vec![SourceMapEntry {
            file: 0, source_file: "spec.emath".into(), source_start: 0, source_end: 11, semantic_node: "y".into(),
            plan_node: Some("plan-0".into()), generated_file: "src/lib.rs".into(), generated_start: 4, generated_end: 9, generated_symbol: Some("Y".into()),
        }],
    }
}

#[test]
fn probe() {
    let mut p = Probe::new("schema ids isolate writer shapes and refuse cross-loads");
    p.case("artifact-map", |p| {
        let map = sample_source_map();
        let doc = write_source_map(&map);
        p.contains("schema", &doc, "\"schema\": \"emath.source-map\"");
        match source_map_from_json(&doc) {
            Ok(parsed) => p.eq("round-trip", parsed, map),
            Err(e) => p.fail("round-trip", format!("must parse: {e:?}")),
        };
    });
    p.case("genesis-map", |p| {
        let source = "/tmp/genesis/exa\"mple.emath";
        let doc = write_generated_crate_source_map(source, &["Cargo.toml".to_string(), "src/lib.rs".to_string()]);
        p.contains("schema", &doc, "\"schema\": \"emath.generated-crate-source-map\"");
        p.contains("kind", &doc, "\"kind\": \"parametric-world\"");
        match emath_artifact::generated_crate_source_map_from_json(&doc) {
            Ok(parsed) => {
                p.eq("schema", parsed.schema, SchemaId("emath.generated-crate-source-map".into()));
                p.eq("source", parsed.source, source.to_string());
                p.eq("entries", parsed.entries, vec![
                    GeneratedCrateSourceMapEntry { generated: "Cargo.toml".into(), source: source.into(), kind: "parametric-world".into() },
                    GeneratedCrateSourceMapEntry { generated: "src/lib.rs".into(), source: source.into(), kind: "parametric-world".into() },
                ]);
            }
            Err(e) => {
                p.fail("parse", format!("must parse: {e:?}"));
            }
        }
        p.demand("cross-refused", source_map_from_json(&doc).is_err(), "genesis bytes must be refused by artifact reader");
    });
    p.case("cross-other-way", |p| {
        p.demand("refused", emath_artifact::generated_crate_source_map_from_json(&write_source_map(&sample_source_map())).is_err(), "artifact bytes must not load as genesis map");
    });
    p.case("plan", |p| {
        let plan = PlanRecord {
            schema: SchemaId("emath.resolution-plan".into()), plan_id: content_id_of_str("schema-lanes-plan"),
            goal: 0, policy: "native-deterministic".into(), artifact_class: "native".into(),
            operations: vec![OperationRecord { node: 0, operation: "package".into(), dependencies: vec![], fallback: None }],
            excluded_candidates: vec![("phase2.expression".into(), "not installed".into())],
        };
        let doc = write_resolution_plan(&plan);
        p.contains("schema", &doc, "\"schema\": \"emath.resolution-plan\"");
        match emath_artifact::plan_from_json(&doc) {
            Ok(parsed) => p.eq("round-trip", parsed, plan),
            Err(e) => p.fail("round-trip", format!("must parse: {e:?}")),
        };
    });
    p.finish();
}
