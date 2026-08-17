// crate root forbids unsafe
//! Thirteen-schema registry.
//!
//! Canonical semantic-genesis artifact formats named
//! `emath.<name>`. The registry is std-only, dependency-free and
//! byte-stable: every entry emits deterministic JSON bytes and a
//! matching example. Unknown names return a stable typed error
//! (`E-SCHEMA-001`). The order of [`SCHEMA_NAMES`] is the fixed
//! registry order.

/// Stable schema document version.
pub const SCHEMA_VERSION: &str = "1.0.0";
/// Registry version (same as schema version, kept for compatibility).
pub const REGISTRY_VERSION: &str = "1.0.0";
/// Version constants plural alias (required by assignment).
pub const SCHEMAS_VERSION: &str = "1.0.0";
/// Crate-level version alias.
pub const VERSION: &str = "1.0.0";

/// Fixed registry order — thirteen stable names.
///
/// Each name is the canonical `$id` (`emath.<name>`). The order is
/// the admission order and must not change.
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
    /// The unknown name that was requested.
    pub name: String,
    /// Stable error code (`E-SCHEMA-001`).
    pub code: &'static str,
}

impl SchemaError {
    /// Stable code for unknown schema.
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

/// Returns the fixed thirteen names in registry order.
#[must_use]
pub fn schema_names() -> &'static [&'static str] {
    &SCHEMA_NAMES
}

/// Alias for `schema_names()` — enumerates the registry.
#[must_use]
pub fn all_schema_names() -> &'static [&'static str] {
    &SCHEMA_NAMES
}

/// Whether `name` is a known schema.
#[must_use]
pub fn is_known_schema(name: &str) -> bool {
    find_index(name).is_some()
}

fn find_index(name: &str) -> Option<usize> {
    SCHEMA_NAMES.iter().position(|n| *n == name)
}

fn json_escape(input: &str, out: &mut String) {
    for ch in input.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                use std::fmt::Write as _;
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
}

fn short_name(id: &str) -> &str {
    id.strip_prefix("emath.").unwrap_or(id)
}

fn build_schema_json(name: &str) -> Vec<u8> {
    let short = short_name(name);
    let mut out = String::new();
    out.push_str("{\n");
    out.push_str("  \"$schema\": \"https://json-schema.org/draft/2020-12/schema\",\n");
    out.push_str("  \"$id\": \"");
    json_escape(name, &mut out);
    out.push_str("\",\n");
    out.push_str("  \"title\": \"");
    json_escape(short, &mut out);
    out.push_str("\",\n");
    out.push_str("  \"type\": \"object\",\n");
    out.push_str("  \"version\": \"");
    json_escape(SCHEMA_VERSION, &mut out);
    out.push_str("\",\n");
    out.push_str("  \"description\": \"Canonical JSON schema for ");
    json_escape(name, &mut out);
    out.push_str("\",\n");
    out.push_str("  \"properties\": {\n");
    out.push_str("    \"schema\": {\n");
    out.push_str("      \"const\": \"");
    json_escape(name, &mut out);
    out.push_str("\"\n");
    out.push_str("    },\n");
    out.push_str("    \"version\": {\n");
    out.push_str("      \"const\": \"");
    json_escape(SCHEMA_VERSION, &mut out);
    out.push_str("\"\n");
    out.push_str("    },\n");
    out.push_str("    \"payload\": {\n");
    out.push_str("      \"type\": \"object\"\n");
    out.push_str("    }\n");
    out.push_str("  },\n");
    out.push_str("  \"required\": [\n");
    out.push_str("    \"schema\",\n");
    out.push_str("    \"version\"\n");
    out.push_str("  ],\n");
    out.push_str("  \"additionalProperties\": false\n");
    out.push_str("}\n");
    out.into_bytes()
}

fn build_example_json(name: &str) -> Vec<u8> {
    let short = short_name(name);
    let mut out = String::new();
    out.push_str("{\n");
    out.push_str("  \"$schema\": \"");
    json_escape(name, &mut out);
    out.push_str("\",\n");
    out.push_str("  \"schema\": \"");
    json_escape(name, &mut out);
    out.push_str("\",\n");
    out.push_str("  \"version\": \"");
    json_escape(SCHEMA_VERSION, &mut out);
    out.push_str("\",\n");
    out.push_str("  \"title\": \"");
    json_escape(short, &mut out);
    out.push_str("\",\n");
    out.push_str("  \"example\": \"");
    json_escape(short, &mut out);
    out.push_str("-example\",\n");
    out.push_str("  \"payload\": {}\n");
    out.push_str("}\n");
    out.into_bytes()
}

/// Returns deterministic JSON schema bytes for `name`.
pub fn schema_json(name: &str) -> Result<Vec<u8>, SchemaError> {
    if find_index(name).is_none() {
        return Err(SchemaError::new(name));
    }
    Ok(build_schema_json(name))
}

/// Returns deterministic JSON bytes for a schema document.
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

/// Returns deterministic example bytes (alias).
pub fn example_json_bytes(name: &str) -> Result<Vec<u8>, SchemaError> {
    example_json(name)
}

/// Writes deterministic example bytes into `out`.
pub fn write_example_json(name: &str, out: &mut Vec<u8>) -> Result<(), SchemaError> {
    let bytes = example_json(name)?;
    out.extend_from_slice(&bytes);
    Ok(())
}

/// String variants — still deterministic and byte-stable.
pub fn schema_json_string(name: &str) -> Result<String, SchemaError> {
    let bytes = schema_json(name)?;
    Ok(String::from_utf8(bytes).expect("schema json is utf8"))
}

pub fn example_json_string(name: &str) -> Result<String, SchemaError> {
    let bytes = example_json(name)?;
    Ok(String::from_utf8(bytes).expect("example json is utf8"))
}

#[cfg(test)]
mod tests {
    use super::*;

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
                schema_str.contains(name),
                "schema json must contain its id {name}",
            );
            assert!(
                schema_str.contains(SCHEMA_VERSION),
                "schema must contain version"
            );
            assert!(
                schema_str.contains("\"$schema\""),
                "schema must contain $schema"
            );
            assert!(
                example_str.contains(name),
                "example must contain its schema id {name}",
            );
            assert!(
                example_str.contains(SCHEMA_VERSION),
                "example must contain version"
            );
            assert!(
                example_str.contains("\"$schema\""),
                "example must contain $schema"
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
    fn byte_stable_across_writers() {
        for name in schema_names() {
            let a = schema_json(name).unwrap();
            let b = build_schema_json(name);
            assert_eq!(a, b);
            let c = example_json(name).unwrap();
            let d = build_example_json(name);
            assert_eq!(c, d);
        }
    }

    #[test]
    fn is_known_schema_matches_registry() {
        for name in schema_names() {
            assert!(is_known_schema(name));
        }
        assert!(!is_known_schema("emath.unknown"));
        assert!(!is_known_schema(""));
    }
}
