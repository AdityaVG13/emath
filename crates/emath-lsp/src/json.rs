//! Minimal deterministic JSON for the LSP skeleton.
//!
//! Objects render with keys in sorted order (via `BTreeMap`), strings use
//! the default escaping table, and numbers render either as integers or as
//! Rust's deterministic `f64` display form. No third-party dependencies.

use std::collections::BTreeMap;
use std::fmt::Write as _;

/// A parsed or constructed JSON value.
#[derive(Debug, Clone, PartialEq)]
pub enum JsonValue {
    /// JSON null.
    Null,
    /// JSON boolean.
    Bool(bool),
    /// JSON integer (LSP ids, positions, versions).
    Number(i64),
    /// JSON float (accepted from clients; rendered deterministically).
    Float(f64),
    /// JSON string.
    String(String),
    /// JSON array.
    Array(Vec<JsonValue>),
    /// JSON object; insertion order is not preserved (keys sort).
    Object(BTreeMap<String, JsonValue>),
}

impl JsonValue {
    /// Renders the value as compact deterministic JSON.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = String::new();
        self.write(&mut out);
        out
    }

    fn write(&self, out: &mut String) {
        match self {
            Self::Null => out.push_str("null"),
            Self::Bool(value) => out.push_str(if *value { "true" } else { "false" }),
            Self::Number(value) => out.push_str(&value.to_string()),
            Self::Float(value) => out.push_str(&value.to_string()),
            Self::String(value) => write_string(value, out),
            Self::Array(values) => {
                out.push('[');
                for (index, value) in values.iter().enumerate() {
                    if index > 0 {
                        out.push(',');
                    }
                    value.write(out);
                }
                out.push(']');
            }
            Self::Object(fields) => {
                out.push('{');
                for (index, (key, value)) in fields.iter().enumerate() {
                    if index > 0 {
                        out.push(',');
                    }
                    write_string(key, out);
                    out.push(':');
                    value.write(out);
                }
                out.push('}');
            }
        }
    }

    /// Parses a JSON document.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut parser = Parser {
            bytes: text.as_bytes(),
            pos: 0,
        };
        let value = parser.parse_value()?;
        parser.skip_ws();
        if parser.pos != parser.bytes.len() {
            return Err(format!("trailing content at {}", parser.pos));
        }
        Ok(value)
    }

    /// Returns the value for `key` in an object.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Self> {
        match self {
            Self::Object(fields) => fields.get(key),
            _ => None,
        }
    }

    /// Returns the string value for `key` in an object.
    #[must_use]
    pub fn get_str(&self, key: &str) -> Option<&str> {
        match self.get(key) {
            Some(Self::String(value)) => Some(value),
            _ => None,
        }
    }

    /// Returns the integer value for `key` in an object.
    #[must_use]
    pub fn get_int(&self, key: &str) -> Option<i64> {
        match self.get(key) {
            Some(Self::Number(value)) => Some(*value),
            _ => None,
        }
    }

    /// Returns true for a JSON null.
    #[must_use]
    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }
}

/// Renders a JSON string literal.
fn write_string(text: &str, out: &mut String) {
    out.push('"');
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
    out.push('"');
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl Parser<'_> {
    fn skip_ws(&mut self) {
        while let Some(byte) = self.bytes.get(self.pos) {
            if byte.is_ascii_whitespace() {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn eat(&mut self, expected: &str) -> bool {
        if self.bytes[self.pos..].starts_with(expected.as_bytes()) {
            self.pos += expected.len();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, expected: &str) -> Result<(), String> {
        if self.eat(expected) {
            Ok(())
        } else {
            Err(format!("expected `{expected}` at {}", self.pos))
        }
    }

    fn parse_value(&mut self) -> Result<JsonValue, String> {
        self.skip_ws();
        let Some(byte) = self.peek() else {
            return Err("unexpected end of input".into());
        };
        match byte {
            b'n' => {
                self.expect("null")?;
                Ok(JsonValue::Null)
            }
            b't' => {
                self.expect("true")?;
                Ok(JsonValue::Bool(true))
            }
            b'f' => {
                self.expect("false")?;
                Ok(JsonValue::Bool(false))
            }
            b'"' => self.parse_string(),
            b'[' => self.parse_array(),
            b'{' => self.parse_object(),
            b'-' | b'0'..=b'9' => self.parse_number(),
            other => Err(format!("unexpected byte 0x{other:02x} at {}", self.pos)),
        }
    }

    fn parse_string(&mut self) -> Result<JsonValue, String> {
        self.expect("\"")?;
        let mut value = String::new();
        loop {
            let Some(byte) = self.peek() else {
                return Err("unterminated string".into());
            };
            self.pos += 1;
            match byte {
                b'"' => return Ok(JsonValue::String(value)),
                b'\\' => {
                    let Some(escaped) = self.peek() else {
                        return Err("unterminated escape".into());
                    };
                    self.pos += 1;
                    match escaped {
                        b'"' => value.push('"'),
                        b'\\' => value.push('\\'),
                        b'/' => value.push('/'),
                        b'n' => value.push('\n'),
                        b'r' => value.push('\r'),
                        b't' => value.push('\t'),
                        b'b' => value.push('\u{0008}'),
                        b'f' => value.push('\u{000c}'),
                        b'u' => {
                            let mut code = 0u32;
                            for _ in 0..4 {
                                let Some(hex) = self.peek() else {
                                    return Err("unterminated unicode escape".into());
                                };
                                self.pos += 1;
                                let digit = match hex {
                                    b'0'..=b'9' => u32::from(hex - b'0'),
                                    b'a'..=b'f' => u32::from(hex - b'a') + 10,
                                    b'A'..=b'F' => u32::from(hex - b'A') + 10,
                                    _ => return Err("invalid unicode escape digit".into()),
                                };
                                code = code * 16 + digit;
                            }
                            let Some(ch) = char::from_u32(code) else {
                                return Err("invalid unicode code point".into());
                            };
                            value.push(ch);
                        }
                        _ => return Err("unknown escape".into()),
                    }
                }
                _other => {
                    let rest = std::str::from_utf8(&self.bytes[self.pos - 1..])
                        .map_err(|_| "invalid utf-8".to_string())?;
                    let ch = rest.chars().next().expect("non-empty slice");
                    value.push(ch);
                    self.pos += ch.len_utf8() - 1;
                }
            }
        }
    }

    fn parse_array(&mut self) -> Result<JsonValue, String> {
        self.expect("[")?;
        let mut values = Vec::new();
        self.skip_ws();
        if self.eat("]") {
            return Ok(JsonValue::Array(values));
        }
        loop {
            values.push(self.parse_value()?);
            self.skip_ws();
            if self.eat(",") {
                continue;
            }
            self.expect("]")?;
            return Ok(JsonValue::Array(values));
        }
    }

    fn parse_object(&mut self) -> Result<JsonValue, String> {
        self.expect("{")?;
        let mut fields = BTreeMap::new();
        self.skip_ws();
        if self.eat("}") {
            return Ok(JsonValue::Object(fields));
        }
        loop {
            self.skip_ws();
            let JsonValue::String(key) = self.parse_value()? else {
                return Err("object key must be a string".into());
            };
            self.skip_ws();
            self.expect(":")?;
            let value = self.parse_value()?;
            fields.insert(key, value);
            self.skip_ws();
            if self.eat(",") {
                continue;
            }
            self.expect("}")?;
            return Ok(JsonValue::Object(fields));
        }
    }

    fn parse_number(&mut self) -> Result<JsonValue, String> {
        self.skip_ws();
        let start = self.pos;
        if self.eat("-") {
            self.skip_ws();
        }
        while self.peek().is_some_and(|byte| {
            byte.is_ascii_digit()
                || byte == b'.'
                || byte == b'e'
                || byte == b'E'
                || byte == b'+'
                || byte == b'-'
        }) {
            self.pos += 1;
        }
        let text = std::str::from_utf8(&self.bytes[start..self.pos])
            .map_err(|_| "invalid number".to_string())?;
        if text.is_empty() {
            return Err("empty number".into());
        }
        if text == "-" {
            return Err("invalid number".into());
        }
        if text.contains(['.', 'e', 'E']) {
            text.parse::<f64>()
                .map(JsonValue::Float)
                .map_err(|_| format!("invalid float `{text}`"))
        } else {
            text.parse::<i64>()
                .map(JsonValue::Number)
                .map_err(|_| format!("invalid integer `{text}`"))
        }
    }
}

/// Parses a JSON-RPC request object into its parts.
pub fn parse_request(text: &str) -> Result<(Option<i64>, String, JsonValue), String> {
    let value = JsonValue::parse(text)?;
    let Some(method) = value.get_str("method") else {
        return Err("request missing `method`".into());
    };
    let id = value
        .get("id")
        .map(|id| match id {
            JsonValue::Number(number) => Ok(*number),
            JsonValue::Null => Ok(0),
            JsonValue::String(text) => text.parse::<i64>().map_err(|_| "string id".to_string()),
            _ => Err("invalid id".into()),
        })
        .transpose()?;
    let params = value.get("params").cloned().unwrap_or(JsonValue::Null);
    Ok((id, method.to_string(), params))
}
