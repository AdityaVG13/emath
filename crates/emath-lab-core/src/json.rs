//! Minimal deterministic JSON for lab artifacts (std-only, in-tree).
//!
//! Writer: keys sorted, numbers via Rust's roundtrip `Display` →
//! byte-identical text. Parser: recursive-descent over objects, arrays,
//! strings (standard escapes + `\uXXXX`), numbers, booleans, null.

use std::fmt;
use std::fmt::Write as _;

/// A JSON value in the lab subset.
#[derive(Clone, Debug, PartialEq)]
pub enum JsonValue {
    /// `null`.
    Null,
    /// Boolean.
    Bool(bool),
    /// String.
    String(String),
    /// Number (f64; integral values round-trip exactly below `2^53`).
    Number(f64),
    /// Array.
    Array(Vec<JsonValue>),
    /// Object (`key -> value` pairs; sorted on write).
    Object(Vec<(String, JsonValue)>),
}

/// JSON read/write failure (wrapped by callers into `LabError`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonError {
    /// Message.
    pub message: String,
}

impl JsonError {
    #[must_use]
    fn at(message: impl Into<String>, position: usize) -> Self {
        Self {
            message: format!("{} at byte {position}", message.into()),
        }
    }
}

impl fmt::Display for JsonError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for JsonError {}

/// Deterministic JSON string for a value: object keys sorted, no whitespace.
#[must_use]
pub fn write(value: &JsonValue) -> String {
    match value {
        JsonValue::Null => "null".to_string(),
        JsonValue::Bool(flag) => {
            if *flag {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
        JsonValue::String(text) => write_string(text),
        JsonValue::Number(number) => format!("{number}"),
        JsonValue::Array(items) => {
            let body: Vec<String> = items.iter().map(write).collect();
            format!("[{}]", body.join(","))
        }
        JsonValue::Object(fields) => {
            let mut sorted = fields.clone();
            sorted.sort_by(|left, right| left.0.cmp(&right.0));
            let body: Vec<String> = sorted
                .iter()
                .map(|(key, value)| format!("{}:{}", write_string(key), write(value)))
                .collect();
            format!("{{{}}}", body.join(","))
        }
    }
}

fn write_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            control if (control as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", control as u32);
            }
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// Maximum parser nesting depth; deeper documents refuse so adversarial
/// inputs cannot exhaust the stack.
const MAX_DEPTH: usize = 128;

/// Parses a JSON document into a value.
pub fn parse(text: &str) -> Result<JsonValue, JsonError> {
    let mut parser = Parser {
        bytes: text.as_bytes(),
        position: 0,
    };
    parser.skip_whitespace();
    let value = parser.value_with_depth(0)?;
    parser.skip_whitespace();
    if parser.position != parser.bytes.len() {
        return Err(JsonError::at("trailing content", parser.position));
    }
    Ok(value)
}

struct Parser<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.position).copied()
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.position += 1;
        }
    }

    fn value_with_depth(&mut self, depth: usize) -> Result<JsonValue, JsonError> {
        if depth > MAX_DEPTH {
            return Err(JsonError::at("nesting depth limit exceeded", self.position));
        }
        self.skip_whitespace();
        match self.peek() {
            Some(b'{') => self.object_with_depth(depth),
            Some(b'[') => self.array_with_depth(depth),
            Some(b'"') => self.string().map(JsonValue::String),
            Some(b't') => self.literal("true").map(|()| JsonValue::Bool(true)),
            Some(b'f') => self.literal("false").map(|()| JsonValue::Bool(false)),
            Some(b'n') => self.literal("null").map(|()| JsonValue::Null),
            Some(byte) if byte == b'-' || byte.is_ascii_digit() => self.number(),
            Some(other) => Err(JsonError::at(
                format!("unexpected byte {}", char::from(other)),
                self.position,
            )),
            None => Err(JsonError::at("unexpected end of input", self.position)),
        }
    }

    fn literal(&mut self, expected: &str) -> Result<(), JsonError> {
        let start = self.position;
        if self.bytes[self.position..].starts_with(expected.as_bytes()) {
            self.position += expected.len();
            Ok(())
        } else {
            Err(JsonError::at(format!("expected literal {expected}"), start))
        }
    }

    fn object_with_depth(&mut self, depth: usize) -> Result<JsonValue, JsonError> {
        self.position += 1; // '{'
        let mut fields = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(b'}') {
            self.position += 1;
            return Ok(JsonValue::Object(fields));
        }
        loop {
            self.skip_whitespace();
            let key = self.string()?;
            self.skip_whitespace();
            if self.peek() != Some(b':') {
                return Err(JsonError::at("expected ':'", self.position));
            }
            self.position += 1;
            let value = self.value_with_depth(depth + 1)?;
            fields.push((key, value));
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => self.position += 1,
                Some(b'}') => {
                    self.position += 1;
                    return Ok(JsonValue::Object(fields));
                }
                _ => return Err(JsonError::at("expected ',' or '}'", self.position)),
            }
        }
    }

    fn array_with_depth(&mut self, depth: usize) -> Result<JsonValue, JsonError> {
        self.position += 1; // '['
        let mut items = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(b']') {
            self.position += 1;
            return Ok(JsonValue::Array(items));
        }
        loop {
            items.push(self.value_with_depth(depth + 1)?);
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => self.position += 1,
                Some(b']') => {
                    self.position += 1;
                    return Ok(JsonValue::Array(items));
                }
                _ => return Err(JsonError::at("expected ',' or ']'", self.position)),
            }
        }
    }

    fn string(&mut self) -> Result<String, JsonError> {
        if self.peek() != Some(b'"') {
            return Err(JsonError::at("expected string", self.position));
        }
        self.position += 1;
        let mut out = Vec::new();
        loop {
            let byte = self
                .peek()
                .ok_or_else(|| JsonError::at("unterminated string", self.position))?;
            match byte {
                b'"' => {
                    self.position += 1;
                    return String::from_utf8(out)
                        .map_err(|_| JsonError::at("string is not valid UTF-8", self.position));
                }
                b'\\' => {
                    self.position += 1;
                    let escape = self
                        .peek()
                        .ok_or_else(|| JsonError::at("unterminated escape", self.position))?;
                    self.position += 1;
                    match escape {
                        b'"' => out.push(b'"'),
                        b'\\' => out.push(b'\\'),
                        b'/' => out.push(b'/'),
                        b'b' => out.push(0x08),
                        b'f' => out.push(0x0c),
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'u' => {
                            let codepoint = self.hex4()?;
                            let character = char::from_u32(codepoint).ok_or_else(|| {
                                JsonError::at(
                                    format!("invalid code point \\u{codepoint:04x}"),
                                    self.position,
                                )
                            })?;
                            let mut encoded = [0_u8; 4];
                            out.extend_from_slice(character.encode_utf8(&mut encoded).as_bytes());
                        }
                        other => {
                            return Err(JsonError::at(
                                format!("invalid escape \\{}", char::from(other)),
                                self.position,
                            ));
                        }
                    }
                }
                control if control < 0x20 => {
                    return Err(JsonError::at("control character in string", self.position));
                }
                other => {
                    out.push(other);
                    self.position += 1;
                }
            }
        }
    }

    fn hex4(&mut self) -> Result<u32, JsonError> {
        let start = self.position;
        let mut codepoint = 0_u32;
        for _ in 0..4 {
            let byte = self
                .peek()
                .ok_or_else(|| JsonError::at("truncated \\u escape", self.position))?;
            let digit = match byte {
                b'0'..=b'9' => u32::from(byte - b'0'),
                b'a'..=b'f' => u32::from(byte - b'a') + 10,
                b'A'..=b'F' => u32::from(byte - b'A') + 10,
                _ => {
                    return Err(JsonError::at(
                        "invalid hex digit in \\u escape",
                        self.position,
                    ));
                }
            };
            codepoint = codepoint * 16 + digit;
            self.position += 1;
        }
        if (0xD800..=0xDFFF).contains(&codepoint) {
            return Err(JsonError::at(
                "surrogate code point in \\u escape (unsupported)",
                start,
            ));
        }
        Ok(codepoint)
    }

    fn number(&mut self) -> Result<JsonValue, JsonError> {
        let start = self.position;
        if self.peek() == Some(b'-') {
            self.position += 1;
        }
        if !self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
            return Err(JsonError::at("invalid number", start));
        }
        while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
            self.position += 1;
        }
        if self.peek() == Some(b'.') {
            self.position += 1;
            if !self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
                return Err(JsonError::at("invalid fraction", self.position));
            }
            while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
                self.position += 1;
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.position += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.position += 1;
            }
            if !self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
                return Err(JsonError::at("invalid exponent", self.position));
            }
            while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
                self.position += 1;
            }
        }
        let text = std::str::from_utf8(&self.bytes[start..self.position])
            .map_err(|_| JsonError::at("number is not ASCII", start))?;
        text.parse::<f64>()
            .map(JsonValue::Number)
            .map_err(|_| JsonError::at("number out of range", start))
    }
}
