//! Schema-registry: thirteen schemas, stable versions, byte-stable JSON,
//! pairwise distinct bodies, closed-world emitters, E-SCHEMA-001 for
//! unknown names.

use emath_schema::{
    REGISTRY_VERSION, SCHEMAS_VERSION, SCHEMA_NAMES, SCHEMA_VERSION, VERSION, SchemaError,
    example_json, example_json_bytes, example_json_string, is_known_schema, schema_json,
    schema_json_bytes, schema_json_string, schema_names, write_example_json, write_schema_json,
};
use emath_test_harness::Probe;

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

#[test]
fn schema_registry() {
    let mut p = Probe::new(
        "thirteen named schemas, v1.0.0, byte-stable JSON, pairwise distinct, E-SCHEMA-001 for unknown",
    );
    p.case("enumerate", |p| {
        let names = schema_names();
        p.eq("len", names.len(), 13);
        p.eq("order", names, &SCHEMA_NAMES[..]);
        p.eq("stable", schema_names(), names);
        let mut seen = std::collections::BTreeSet::new();
        for n in names {
            p.demand(format!("unique-{n}"), seen.insert(*n), format!("duplicate {n}"));
        }
    });
    p.case("versions", |p| {
        p.eq("schema", SCHEMA_VERSION, "1.0.0");
        p.eq("registry", REGISTRY_VERSION, "1.0.0");
        p.eq("schemas", SCHEMAS_VERSION, "1.0.0");
        p.eq("version", VERSION, "1.0.0");
    });
    p.case("byte-stable-json", |p| {
        for name in schema_names() {
            let schema = schema_json(name).expect("known schema");
            let example = example_json(name).expect("known example");
            p.eq(format!("{name}/schema-stable"), schema.clone(), schema_json(name).unwrap());
            p.eq(format!("{name}/example-stable"), example.clone(), example_json(name).unwrap());
            p.demand(format!("{name}/schema-brace"), schema.starts_with(b"{"), "schema starts with {");
            p.demand(format!("{name}/schema-nl"), schema.ends_with(b"\n"), "schema newline");
            p.demand(format!("{name}/example-brace"), example.starts_with(b"{"), "example starts with {");
            let schema_str = String::from_utf8(schema.clone()).unwrap();
            let example_str = String::from_utf8(example.clone()).unwrap();
            p.contains(format!("{name}/id"), &schema_str, &format!("\"$id\": \"{name}\""));
            p.contains(format!("{name}/ver"), &schema_str, SCHEMA_VERSION);
            p.contains(format!("{name}/schema-key"), &schema_str, "\"$schema\"");
            p.contains(
                format!("{name}/example-schema"),
                &example_str,
                &format!("\"$schema\": \"{name}\""),
            );
            p.eq(format!("{name}/schema-bytes"), schema.clone(), schema_json_bytes(name).unwrap());
            p.eq(format!("{name}/example-bytes"), example.clone(), example_json_bytes(name).unwrap());
            let mut buf = Vec::new();
            write_schema_json(name, &mut buf).unwrap();
            p.eq(format!("{name}/write-schema"), buf, schema.clone());
            let mut buf2 = Vec::new();
            write_example_json(name, &mut buf2).unwrap();
            p.eq(format!("{name}/write-example"), buf2, example.clone());
            p.eq(
                format!("{name}/schema-string"),
                schema_json_string(name).unwrap().as_bytes(),
                schema.as_slice(),
            );
            p.eq(
                format!("{name}/example-string"),
                example_json_string(name).unwrap().as_bytes(),
                example.as_slice(),
            );
        }
    });
    p.case("pairwise-distinct", |p| {
        let docs: Vec<Vec<u8>> = schema_names()
            .iter()
            .map(|name| schema_json(name).expect("known schema"))
            .collect();
        for (i, left) in docs.iter().enumerate() {
            for (j, right) in docs.iter().enumerate().skip(i + 1) {
                p.ne(
                    format!("{}-vs-{}", SCHEMA_NAMES[i], SCHEMA_NAMES[j]),
                    left,
                    right,
                );
            }
        }
    });
    p.case("not-one-template", |p| {
        let source = without_identity("emath.source-artifact");
        let forest = without_identity("emath.parse-forest");
        let receipt = without_identity("emath.answer-receipt");
        let portfolio = without_identity("emath.interpretation-portfolio");
        let envelope = without_identity("emath.meaning-lock");
        p.ne("source-forest", source.clone(), forest.clone());
        p.ne("source-receipt", source.clone(), receipt.clone());
        p.ne("source-envelope", source, envelope.clone());
        p.ne("forest-receipt", forest.clone(), receipt.clone());
        p.ne("forest-envelope", forest, envelope.clone());
        p.ne("receipt-portfolio", receipt, portfolio.clone());
        p.ne("portfolio-envelope", portfolio, envelope);
    });
    p.case("source-artifact-fields", |p| {
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
            p.contains(field, &schema, &format!("\"{field}\""));
        }
        p.contains("closed", &schema, "\"additionalProperties\": false");
        p.demand("no-payload", !schema.contains("\"payload\""), "source-artifact must not invent payload");
    });
    p.case("parse-forest-fields", |p| {
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
            p.contains(field, &schema, &format!("\"{field}\""));
        }
        p.contains("closed", &schema, "\"additionalProperties\": false");
        p.contains("bounded", &schema, "\"const\": \"bounded-holes\"");
    });
    p.case("answer-receipt-fields", |p| {
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
            p.contains(field, &schema, &format!("\"{field}\""));
        }
        p.contains("closed", &schema, "\"additionalProperties\": false");
    });
    p.case("envelope-open", |p| {
        let schema = schema_json_string("emath.meaning-lock").unwrap();
        p.contains("open", &schema, "\"additionalProperties\": true");
        p.demand("no-payload", !schema.contains("\"payload\""), "envelope must not invent payload");
        p.demand("no-world", !schema.contains("\"world_name\""), "envelope must not invent world_name");
        p.contains("const", &schema, "\"const\": \"emath.meaning-lock\"");
    });
    p.case("unknown-e-schema-001", |p| {
        let unknown = "emath.unknown";
        match schema_json(unknown) {
            Ok(_) => { p.fail("err", "unknown must refuse"); }
            Err(err) => {
                p.eq("code()", err.code(), "E-SCHEMA-001");
                p.eq("code", err.code, "E-SCHEMA-001");
                p.eq("name()", err.name(), unknown);
                p.eq("name", err.name.as_str(), unknown);
                let display = format!("{err}");
                p.contains("display-name", &display, unknown);
                p.contains("display-code", &display, "E-SCHEMA-001");
                let _: &dyn std::error::Error = &err;
            }
        }
        match example_json(unknown) {
            Ok(_) => { p.fail("example", "unknown example must refuse"); }
            Err(err2) => {
                p.eq("example-code", err2.code(), "E-SCHEMA-001");
                p.eq("example-name", err2.name(), unknown);
            }
        }
        for bad in ["", "unknown", "emath.parse-forest.x", "EMATH.PARSE-FOREST"] {
            p.demand(format!("schema-{bad}"), schema_json(bad).is_err(), format!("should refuse {bad}"));
            p.demand(format!("example-{bad}"), example_json(bad).is_err(), format!("should refuse {bad}"));
            if let Err(e) = schema_json(bad) {
                p.eq(format!("code-{bad}"), e.code, SchemaError::CODE);
            }
        }
    });
    p.case("is-known", |p| {
        for name in schema_names() {
            p.demand(format!("known-{name}"), is_known_schema(name), "registry name must be known");
        }
        p.demand("unknown", !is_known_schema("emath.unknown"), "unknown");
        p.demand("empty", !is_known_schema(""), "empty");
    });
    p.finish();
}
