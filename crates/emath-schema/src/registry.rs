// crate root forbids unsafe
//! Thirteen-schema registry: deterministic, byte-stable JSON per `emath.<name>`.
//! Unknown names return `E-SCHEMA-001`; [`SCHEMA_NAMES`] order is fixed.

/// Stable schema document version.
pub const SCHEMA_VERSION: &str = "1.0.0";
/// Registry version (same as schema version, kept for compatibility).
pub const REGISTRY_VERSION: &str = "1.0.0";
/// Version constants plural alias (required by assignment).
pub const SCHEMAS_VERSION: &str = "1.0.0";
/// Crate-level version alias.
pub const VERSION: &str = "1.0.0";

/// Fixed registry order — thirteen stable `$id` names; must not change.
pub const SCHEMA_NAMES: [&str; 13] = [
    "emath.source-artifact",
    "emath.parse-forest",
    "emath.symbol-signature",
    "emath.term-ir",
    "emath.world-ir",
    "emath.world-morphism",
    "emath.meaning-lock",
    "emath.agent-world-proposal",
    "emath.answer-receipt",
    "emath.continuation",
    "emath.interpretation-portfolio",
    "emath.math-layout-graph",
    "emath.provenance-receipt",
];

/// Typed refusal for an unknown schema name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchemaError {
    pub name: String,
    pub code: &'static str,
}

impl SchemaError {
    pub const CODE: &'static str = "E-SCHEMA-001";

    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            code: Self::CODE,
        }
    }

    #[must_use]
    pub fn code(&self) -> &'static str {
        self.code
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl std::fmt::Display for SchemaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown schema `{}` ({})", self.name, self.code)
    }
}

impl std::error::Error for SchemaError {}

/// Alias required by assignment — typed unknown-name refusal.
pub type UnknownSchemaError = SchemaError;

#[must_use]
pub fn schema_names() -> &'static [&'static str] {
    &SCHEMA_NAMES
}

/// Alias for `schema_names()`.
#[must_use]
pub fn all_schema_names() -> &'static [&'static str] {
    &SCHEMA_NAMES
}

#[must_use]
pub fn is_known_schema(name: &str) -> bool {
    find_index(name).is_some()
}

fn find_index(name: &str) -> Option<usize> {
    SCHEMA_NAMES.iter().position(|n| *n == name)
}

fn json_escape(input: &str, out: &mut String) {
    use std::fmt::Write as _;
    for ch in input.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
}

fn short_name(id: &str) -> &str {
    id.strip_prefix("emath.").unwrap_or(id)
}

#[derive(Clone, Copy)]
enum JsonType {
    String,
    Integer,
    Number,
}

impl JsonType {
    const fn as_str(self) -> &'static str {
        match self {
            Self::String => "string",
            Self::Integer => "integer",
            Self::Number => "number",
        }
    }
}

#[derive(Clone, Copy)]
struct FieldDef {
    name: &'static str,
    value: ValueSpec,
    required: bool,
}

#[derive(Clone, Copy)]
enum ValueSpec {
    Const(&'static str),
    Type(JsonType),
    StringArray,
    Object {
        fields: &'static [FieldDef],
        additional: bool,
    },
    ObjectArray {
        fields: &'static [FieldDef],
        additional: bool,
    },
}

#[derive(Clone, Copy)]
struct ClosedSpec {
    description: &'static str,
    fields: &'static [FieldDef],
}

/// Optional instance annotation; examples include it so `$schema` round-trips.
const SCHEMA_ANNOTATION: FieldDef = FieldDef {
    name: "$schema",
    value: ValueSpec::Type(JsonType::String),
    required: false,
};

const ENVELOPE_DESCRIPTION: &str =
    "No in-tree JSON emitter for this $id; envelope records the schema const only.";

const HOLE_FIELDS: &[FieldDef] = &[
    FieldDef {
        name: "id",
        value: ValueSpec::Type(JsonType::String),
        required: true,
    },
    FieldDef {
        name: "reason",
        value: ValueSpec::Type(JsonType::String),
        required: true,
    },
];

const SCORE_FIELDS: &[FieldDef] = &[
    FieldDef {
        name: "cost",
        value: ValueSpec::Type(JsonType::Number),
        required: true,
    },
    FieldDef {
        name: "complexity",
        value: ValueSpec::Type(JsonType::Number),
        required: true,
    },
    FieldDef {
        name: "evidence",
        value: ValueSpec::Type(JsonType::Number),
        required: true,
    },
    FieldDef {
        name: "utility",
        value: ValueSpec::Type(JsonType::Number),
        required: true,
    },
];

const CANDIDATE_FIELDS: &[FieldDef] = &[
    FieldDef {
        name: "world_id",
        value: ValueSpec::Type(JsonType::String),
        required: true,
    },
    FieldDef {
        name: "name",
        value: ValueSpec::Type(JsonType::String),
        required: true,
    },
    FieldDef {
        name: "answer",
        value: ValueSpec::Type(JsonType::String),
        required: true,
    },
    FieldDef {
        name: "authority",
        value: ValueSpec::Type(JsonType::String),
        required: true,
    },
    FieldDef {
        name: "score",
        value: ValueSpec::Object {
            fields: SCORE_FIELDS,
            additional: false,
        },
        required: true,
    },
    FieldDef {
        name: "provenance",
        value: ValueSpec::Type(JsonType::String),
        required: true,
    },
];

const SOURCE_ARTIFACT_FIELDS: &[FieldDef] = &[
    FieldDef {
        name: "schema",
        value: ValueSpec::Const("emath.source-artifact"),
        required: true,
    },
    FieldDef {
        name: "schema_version",
        value: ValueSpec::Type(JsonType::Integer),
        required: true,
    },
    FieldDef {
        name: "source",
        value: ValueSpec::Type(JsonType::String),
        required: true,
    },
    FieldDef {
        name: "source_hash",
        value: ValueSpec::Type(JsonType::Integer),
        required: true,
    },
    FieldDef {
        name: "byte_len",
        value: ValueSpec::Type(JsonType::Integer),
        required: true,
    },
    FieldDef {
        name: "world_name",
        value: ValueSpec::Type(JsonType::String),
        required: true,
    },
    FieldDef {
        name: "body_text",
        value: ValueSpec::Type(JsonType::String),
        required: true,
    },
    FieldDef {
        name: "glyph_count",
        value: ValueSpec::Type(JsonType::Integer),
        required: true,
    },
    FieldDef {
        name: "glyphs",
        value: ValueSpec::StringArray,
        required: true,
    },
    FieldDef {
        name: "parse_id",
        value: ValueSpec::Type(JsonType::Integer),
        required: true,
    },
];

const PARSE_FOREST_FIELDS: &[FieldDef] = &[
    FieldDef {
        name: "schema",
        value: ValueSpec::Const("emath.parse-forest"),
        required: true,
    },
    FieldDef {
        name: "world_name",
        value: ValueSpec::Type(JsonType::String),
        required: true,
    },
    FieldDef {
        name: "body",
        value: ValueSpec::Type(JsonType::String),
        required: true,
    },
    FieldDef {
        name: "parse_id",
        value: ValueSpec::Type(JsonType::Integer),
        required: true,
    },
    FieldDef {
        name: "ambiguity_count",
        value: ValueSpec::Type(JsonType::Integer),
        required: true,
    },
    FieldDef {
        name: "node_count",
        value: ValueSpec::Type(JsonType::Integer),
        required: true,
    },
    FieldDef {
        name: "holes",
        value: ValueSpec::ObjectArray {
            fields: HOLE_FIELDS,
            additional: false,
        },
        required: true,
    },
    FieldDef {
        name: "canonical_term",
        value: ValueSpec::Type(JsonType::String),
        required: false,
    },
    FieldDef {
        name: "recovery",
        value: ValueSpec::Const("bounded-holes"),
        required: true,
    },
];

const ANSWER_RECEIPT_FIELDS: &[FieldDef] = &[
    FieldDef {
        name: "schema",
        value: ValueSpec::Const("emath.answer-receipt"),
        required: true,
    },
    FieldDef {
        name: "schema_version",
        value: ValueSpec::Type(JsonType::Integer),
        required: true,
    },
    FieldDef {
        name: "receipt_id",
        value: ValueSpec::Type(JsonType::String),
        required: true,
    },
    FieldDef {
        name: "answer_id",
        value: ValueSpec::Type(JsonType::String),
        required: true,
    },
    FieldDef {
        name: "source_hash",
        value: ValueSpec::Type(JsonType::Integer),
        required: true,
    },
    FieldDef {
        name: "parse_id",
        value: ValueSpec::Type(JsonType::Integer),
        required: true,
    },
    FieldDef {
        name: "signature_id",
        value: ValueSpec::Type(JsonType::Integer),
        required: true,
    },
    FieldDef {
        name: "term_id",
        value: ValueSpec::Type(JsonType::Integer),
        required: true,
    },
    FieldDef {
        name: "world_id",
        value: ValueSpec::Type(JsonType::String),
        required: true,
    },
    FieldDef {
        name: "valuation",
        value: ValueSpec::Type(JsonType::String),
        required: true,
    },
    FieldDef {
        name: "provider_locks",
        value: ValueSpec::StringArray,
        required: true,
    },
    FieldDef {
        name: "checker_receipts",
        value: ValueSpec::StringArray,
        required: true,
    },
    FieldDef {
        name: "artifact_hash",
        value: ValueSpec::Type(JsonType::String),
        required: true,
    },
    FieldDef {
        name: "portfolio_hash",
        value: ValueSpec::Type(JsonType::String),
        required: true,
    },
    FieldDef {
        name: "target",
        value: ValueSpec::Type(JsonType::String),
        required: true,
    },
    FieldDef {
        name: "result",
        value: ValueSpec::Type(JsonType::String),
        required: true,
    },
    FieldDef {
        name: "trace_hash",
        value: ValueSpec::Type(JsonType::String),
        required: true,
    },
    FieldDef {
        name: "authority",
        value: ValueSpec::Type(JsonType::String),
        required: true,
    },
    FieldDef {
        name: "vm_schema",
        value: ValueSpec::Type(JsonType::String),
        required: true,
    },
    FieldDef {
        name: "vm_steps",
        value: ValueSpec::Type(JsonType::Integer),
        required: true,
    },
];

const PORTFOLIO_FIELDS: &[FieldDef] = &[
    FieldDef {
        name: "schema",
        value: ValueSpec::Const("emath.interpretation-portfolio"),
        required: true,
    },
    FieldDef {
        name: "portfolio_id",
        value: ValueSpec::Type(JsonType::Integer),
        required: true,
    },
    FieldDef {
        name: "candidates",
        value: ValueSpec::ObjectArray {
            fields: CANDIDATE_FIELDS,
            additional: false,
        },
        required: true,
    },
];

fn closed_spec(name: &str) -> Option<ClosedSpec> {
    match name {
        "emath.source-artifact" => Some(ClosedSpec {
            description: "Sealed genesis source artifact (emath-cli genesis_cmd).",
            fields: SOURCE_ARTIFACT_FIELDS,
        }),
        "emath.parse-forest" => Some(ClosedSpec {
            description: "Bounded parse forest (emath-genesis ParseForest::canonical_json).",
            fields: PARSE_FOREST_FIELDS,
        }),
        "emath.answer-receipt" => Some(ClosedSpec {
            description: "SG-09 answer receipt (emath-cli genesis_cmd).",
            fields: ANSWER_RECEIPT_FIELDS,
        }),
        "emath.interpretation-portfolio" => Some(ClosedSpec {
            description: "Interpretation portfolio (emath-cli genesis_cmd).",
            fields: PORTFOLIO_FIELDS,
        }),
        _ => None,
    }
}

fn push_indent(out: &mut String, level: usize) {
    for _ in 0..level {
        out.push_str("  ");
    }
}

fn write_properties<'a, I>(out: &mut String, fields: I, indent: usize)
where
    I: Iterator<Item = &'a FieldDef>,
{
    push_indent(out, indent);
    out.push_str("\"properties\": {\n");
    let mut first = true;
    for field in fields {
        if !first {
            out.push_str(",\n");
        }
        first = false;
        push_indent(out, indent + 1);
        out.push('"');
        json_escape(field.name, out);
        out.push_str("\": ");
        write_value_spec(out, &field.value, indent + 1);
    }
    out.push('\n');
    push_indent(out, indent);
    out.push('}');
}

fn write_required<'a, I>(out: &mut String, fields: I, indent: usize)
where
    I: Iterator<Item = &'a FieldDef>,
{
    push_indent(out, indent);
    out.push_str("\"required\": [\n");
    let mut first = true;
    for field in fields.filter(|field| field.required) {
        if !first {
            out.push_str(",\n");
        }
        first = false;
        push_indent(out, indent + 1);
        out.push('"');
        json_escape(field.name, out);
        out.push('"');
    }
    out.push('\n');
    push_indent(out, indent);
    out.push(']');
}

fn write_object_shape(out: &mut String, fields: &[FieldDef], additional: bool, indent: usize) {
    push_indent(out, indent);
    out.push_str("\"type\": \"object\",\n");
    write_properties(out, fields.iter(), indent);
    out.push_str(",\n");
    write_required(out, fields.iter(), indent);
    out.push_str(",\n");
    push_indent(out, indent);
    out.push_str("\"additionalProperties\": ");
    out.push_str(if additional { "true" } else { "false" });
    out.push('\n');
}

fn write_value_spec(out: &mut String, spec: &ValueSpec, indent: usize) {
    out.push_str("{\n");
    match spec {
        ValueSpec::Const(value) => {
            push_indent(out, indent + 1);
            out.push_str("\"const\": \"");
            json_escape(value, out);
            out.push_str("\"\n");
        }
        ValueSpec::Type(json_type) => {
            push_indent(out, indent + 1);
            out.push_str("\"type\": \"");
            out.push_str(json_type.as_str());
            out.push_str("\"\n");
        }
        ValueSpec::StringArray => {
            push_indent(out, indent + 1);
            out.push_str("\"type\": \"array\",\n");
            push_indent(out, indent + 1);
            out.push_str("\"items\": {\n");
            push_indent(out, indent + 2);
            out.push_str("\"type\": \"string\"\n");
            push_indent(out, indent + 1);
            out.push_str("}\n");
        }
        ValueSpec::Object { fields, additional } => {
            write_object_shape(out, fields, *additional, indent + 1);
        }
        ValueSpec::ObjectArray { fields, additional } => {
            push_indent(out, indent + 1);
            out.push_str("\"type\": \"array\",\n");
            push_indent(out, indent + 1);
            out.push_str("\"items\": {\n");
            write_object_shape(out, fields, *additional, indent + 2);
            push_indent(out, indent + 1);
            out.push_str("}\n");
        }
    }
    push_indent(out, indent);
    out.push('}');
}

fn write_schema_header(out: &mut String, name: &str, description: &str) {
    let short = short_name(name);
    out.push_str("{\n");
    out.push_str("  \"$schema\": \"https://json-schema.org/draft/2020-12/schema\",\n");
    out.push_str("  \"$id\": \"");
    json_escape(name, out);
    out.push_str("\",\n");
    out.push_str("  \"title\": \"");
    json_escape(short, out);
    out.push_str("\",\n");
    out.push_str("  \"type\": \"object\",\n");
    out.push_str("  \"version\": \"");
    json_escape(SCHEMA_VERSION, out);
    out.push_str("\",\n");
    out.push_str("  \"description\": \"");
    json_escape(description, out);
    out.push_str("\",\n");
}

fn build_closed_schema_json(name: &str, spec: &ClosedSpec) -> Vec<u8> {
    let mut out = String::new();
    write_schema_header(&mut out, name, spec.description);
    write_properties(
        &mut out,
        std::iter::once(&SCHEMA_ANNOTATION).chain(spec.fields.iter()),
        1,
    );
    out.push_str(",\n");
    write_required(
        &mut out,
        std::iter::once(&SCHEMA_ANNOTATION).chain(spec.fields.iter()),
        1,
    );
    out.push_str(",\n");
    out.push_str("  \"additionalProperties\": false\n}\n");
    out.into_bytes()
}

fn build_envelope_schema_json(name: &str) -> Vec<u8> {
    let mut out = String::new();
    write_schema_header(&mut out, name, ENVELOPE_DESCRIPTION);
    out.push_str("  \"properties\": {\n");
    out.push_str("    \"$schema\": {\n");
    out.push_str("      \"type\": \"string\"\n");
    out.push_str("    },\n");
    out.push_str("    \"schema\": {\n");
    out.push_str("      \"const\": \"");
    json_escape(name, &mut out);
    out.push_str("\"\n");
    out.push_str("    }\n");
    out.push_str("  },\n");
    out.push_str("  \"required\": [\n");
    out.push_str("    \"schema\"\n");
    out.push_str("  ],\n");
    out.push_str("  \"additionalProperties\": true\n}\n");
    out.into_bytes()
}

fn build_schema_json(name: &str) -> Vec<u8> {
    match closed_spec(name) {
        Some(spec) => build_closed_schema_json(name, &spec),
        None => build_envelope_schema_json(name),
    }
}

fn write_example_value(out: &mut String, spec: &ValueSpec) {
    match spec {
        ValueSpec::Const(value) => {
            out.push('"');
            json_escape(value, out);
            out.push('"');
        }
        ValueSpec::Type(JsonType::String) => out.push_str("\"example\""),
        ValueSpec::Type(JsonType::Integer | JsonType::Number) => out.push('0'),
        ValueSpec::StringArray | ValueSpec::ObjectArray { .. } => out.push_str("[]"),
        ValueSpec::Object { fields, .. } => {
            out.push_str("{\n");
            let mut first = true;
            for field in fields.iter().filter(|field| field.required) {
                if !first {
                    out.push_str(",\n");
                }
                first = false;
                out.push_str("    \"");
                json_escape(field.name, out);
                out.push_str("\": ");
                write_example_value(out, &field.value);
            }
            out.push_str("\n  }");
        }
    }
}

fn build_example_json(name: &str) -> Vec<u8> {
    let mut out = String::new();
    out.push_str("{\n  \"$schema\": \"");
    json_escape(name, &mut out);
    out.push_str("\",\n  \"schema\": \"");
    json_escape(name, &mut out);
    out.push('"');
    if let Some(spec) = closed_spec(name) {
        for field in spec
            .fields
            .iter()
            .filter(|field| field.required && field.name != "schema")
        {
            out.push_str(",\n  \"");
            json_escape(field.name, &mut out);
            out.push_str("\": ");
            write_example_value(&mut out, &field.value);
        }
    }
    out.push_str("\n}\n");
    out.into_bytes()
}

pub fn schema_json(name: &str) -> Result<Vec<u8>, SchemaError> {
    if find_index(name).is_none() {
        return Err(SchemaError::new(name));
    }
    Ok(build_schema_json(name))
}

pub fn schema_json_bytes(name: &str) -> Result<Vec<u8>, SchemaError> {
    schema_json(name)
}

/// Writes deterministic JSON schema bytes into `out`.
pub fn write_schema_json(name: &str, out: &mut Vec<u8>) -> Result<(), SchemaError> {
    let bytes = schema_json(name)?;
    out.extend_from_slice(&bytes);
    Ok(())
}

/// Returns deterministic example bytes for `name`.
pub fn example_json(name: &str) -> Result<Vec<u8>, SchemaError> {
    if find_index(name).is_none() {
        return Err(SchemaError::new(name));
    }
    Ok(build_example_json(name))
}

pub fn example_json_bytes(name: &str) -> Result<Vec<u8>, SchemaError> {
    example_json(name)
}

/// Writes deterministic example bytes into `out`.
pub fn write_example_json(name: &str, out: &mut Vec<u8>) -> Result<(), SchemaError> {
    let bytes = example_json(name)?;
    out.extend_from_slice(&bytes);
    Ok(())
}

pub fn schema_json_string(name: &str) -> Result<String, SchemaError> {
    let bytes = schema_json(name)?;
    // Registry emitters write UTF-8 only; surface corruption as E-SCHEMA-001.
    String::from_utf8(bytes).map_err(|_| SchemaError::new(name))
}

pub fn example_json_string(name: &str) -> Result<String, SchemaError> {
    let bytes = example_json(name)?;
    String::from_utf8(bytes).map_err(|_| SchemaError::new(name))
}

// Registry tests moved to `tests/emath-schema/tests/registry.rs`.
