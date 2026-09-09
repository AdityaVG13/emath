//! Deterministic JSON writer. Std-only; the workspace forbids serde.
//!
//! This is the shared emitter for machine JSON. Artifact schemas, plan
//! inspection, and CLI output all go through this writer so escaping and
//! field order stay identical.

use std::fmt::Write as _;

/// Minimal deterministic JSON writer (order preserving, two-space indent).
pub struct JsonWriter;

impl JsonWriter {
    #[must_use]
    pub fn object() -> JsonObject {
        JsonObject { out: String::new() }
    }
}

pub struct JsonObject {
    out: String,
}

impl JsonObject {
    pub fn field(&mut self, name: &str, value: &str) -> &mut Self {
        if !self.out.is_empty() {
            self.out.push_str(",\n");
        }
        let entry = format!("  {}: {}", json_quote(name), value);
        self.out.push_str(&entry);
        self
    }

    pub fn string(&mut self, name: &str, value: &str) -> &mut Self {
        self.field(name, &json_quote(value))
    }

    pub fn strings(&mut self, name: &str, values: &[String]) -> &mut Self {
        let mut items = Vec::new();
        for value in values {
            items.push(json_quote(value));
        }
        self.field(name, &format!("[{}]", items.join(", ")))
    }

    /// Array of already-serialized JSON objects. `items` are `finish()` bodies
    /// (or other object texts); this crate owns the array brackets so callers
    /// do not concatenate JSON by hand.
    pub fn objects(&mut self, name: &str, items: &[String]) -> &mut Self {
        if items.is_empty() {
            return self.field(name, "[]");
        }
        let mut body = String::from("[\n");
        for (index, item) in items.iter().enumerate() {
            if index > 0 {
                body.push_str(",\n");
            }
            body.push_str(item.trim());
        }
        body.push_str("\n  ]");
        self.field(name, &body)
    }

    pub fn int(&mut self, name: &str, value: u64) -> &mut Self {
        self.field(name, &value.to_string())
    }

    pub fn bool(&mut self, name: &str, value: bool) -> &mut Self {
        self.field(name, if value { "true" } else { "false" })
    }

    pub fn object_field(&mut self, name: &str, body: &str) -> &mut Self {
        self.field(name, body)
    }

    #[must_use]
    pub fn finish(self) -> String {
        format!("{{\n{}\n}}\n", self.out)
    }
}

/// Escape `text` into `out` (no surrounding quotes).
///
/// C0 controls below `0x20` become `\u00xx`. This is the shared table for
/// genesis receipts, registry/plugin descriptors, proof records, and the
/// writer. Callers that use `char::is_control()` or `\b`/`\f` keep their
/// own tables -- those are different contracts.
pub fn json_escape_into(text: &str, out: &mut String) {
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if (ch as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", ch as u32);
            }
            ch => out.push(ch),
        }
    }
}

/// Escape without surrounding quotes.
#[must_use]
pub fn json_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    json_escape_into(text, &mut out);
    out
}

/// JSON string literal with the writer's escaping rules.
#[must_use]
pub fn json_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    json_quote_into(s, &mut out);
    out
}

/// Append a quoted JSON string literal to `out`.
pub fn json_quote_into(s: &str, out: &mut String) {
    out.push('"');
    json_escape_into(s, out);
    out.push('"');
}
